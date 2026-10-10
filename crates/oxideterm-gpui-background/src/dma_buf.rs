use ash::vk;
use oxideterm_background_media::{MediaError, MediaFrame};
use std::{
    os::fd::{AsRawFd, IntoRawFd},
    sync::Arc,
};

fn error(value: impl std::fmt::Display) -> MediaError {
    MediaError::Decode(value.to_string())
}

pub(crate) struct Importer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

struct ImportedBuffer {
    // Keep the wgpu owner alive until the raw Vulkan objects have been destroyed.
    _owner: Arc<wgpu::Device>,
    device: ash::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}

impl Drop for ImportedBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            if self.memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.memory, None);
            }
        }
    }
}

impl Importer {
    pub fn new(context: Box<dyn std::any::Any>) -> Option<Self> {
        let (device, queue) = *context
            .downcast::<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>()
            .ok()?;
        {
            let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }?;
            let extensions = hal.enabled_device_extensions();
            if !extensions.contains(&ash::ext::external_memory_dma_buf::NAME)
                || !extensions.contains(&ash::khr::external_memory_fd::NAME)
            {
                return None;
            }
            let mut properties = vk::ExternalBufferProperties::default();
            unsafe {
                hal.shared_instance()
                    .raw_instance()
                    .get_physical_device_external_buffer_properties(
                        hal.raw_physical_device(),
                        &vk::PhysicalDeviceExternalBufferInfo::default()
                            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
                        &mut properties,
                    );
            }
            if !properties
                .external_memory_properties
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
            {
                return None;
            }
        }
        Some(Self { device, queue })
    }

    pub fn import(
        &self,
        frame: Arc<MediaFrame>,
        lease: Arc<crate::gpu_budget::GpuLease>,
    ) -> Result<Arc<wgpu::TextureView>, MediaError> {
        let native = frame.native.as_ref().ok_or(MediaError::Unsupported)?;
        if native.width > self.device.limits().max_texture_dimension_2d
            || native.height > self.device.limits().max_texture_dimension_2d
        {
            return Err(MediaError::Unsupported);
        }
        let hal = unsafe { self.device.as_hal::<wgpu::hal::api::Vulkan>() }
            .ok_or(MediaError::Unsupported)?;
        let raw = hal.raw_device();
        let mut external = vk::ExternalMemoryBufferCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let buffer = unsafe {
            raw.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(native.byte_len() as u64)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .push_next(&mut external),
                None,
            )
        }
        .map_err(error)?;
        let mut imported = ImportedBuffer {
            _owner: self.device.clone(),
            device: raw.clone(),
            buffer,
            memory: vk::DeviceMemory::null(),
        };
        let requirements = unsafe { raw.get_buffer_memory_requirements(buffer) };
        if requirements.size > native.byte_len() as u64 {
            return Err(MediaError::Unsupported);
        }
        let fd = native.fd.try_clone()?;
        let extension =
            ash::khr::external_memory_fd::Device::new(hal.shared_instance().raw_instance(), raw);
        let mut properties = vk::MemoryFdPropertiesKHR::default();
        unsafe {
            extension.get_memory_fd_properties(
                vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                fd.as_raw_fd(),
                &mut properties,
            )
        }
        .map_err(error)?;
        let bits = properties.memory_type_bits & requirements.memory_type_bits;
        if bits == 0 {
            return Err(MediaError::Unsupported);
        }
        let mut import = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(fd.as_raw_fd());
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().buffer(buffer);
        imported.memory = unsafe {
            raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(native.byte_len() as u64)
                    .memory_type_index(bits.trailing_zeros())
                    .push_next(&mut import)
                    .push_next(&mut dedicated),
                None,
            )
        }
        .map_err(error)?;
        // A successful Vulkan import takes ownership of the duplicated descriptor.
        let _ = fd.into_raw_fd();
        unsafe { raw.bind_buffer_memory(buffer, imported.memory, 0) }.map_err(error)?;

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("DMA video frame"),
            size: wgpu::Extent3d {
                width: native.width,
                height: native.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = Arc::new(texture.create_view(&Default::default()));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("DMA video import"),
            });
        // Initialize through wgpu before the raw copy so its initialization tracker and layouts agree.
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("initialize video texture"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        encoder.transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: &texture,
                selector: None,
                state: wgpu::TextureUses::COPY_DST,
            }),
        );
        let destination =
            unsafe { texture.as_hal::<wgpu::hal::api::Vulkan>() }.ok_or(MediaError::Unsupported)?;
        unsafe {
            encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|encoder| {
                let encoder = encoder.ok_or(MediaError::Unsupported)?;
                let command = encoder.raw_handle();
                let acquire = vk::BufferMemoryBarrier::default()
                    .buffer(buffer)
                    .size(vk::WHOLE_SIZE)
                    .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                    .dst_queue_family_index(hal.queue_family_index())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
                raw.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[acquire],
                    &[],
                );
                let region = vk::BufferImageCopy::default()
                    .buffer_offset(native.offset)
                    .buffer_row_length(native.stride / 4)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: native.width,
                        height: native.height,
                        depth: 1,
                    });
                raw.cmd_copy_buffer_to_image(
                    command,
                    buffer,
                    destination.raw_handle(),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
                let release = vk::BufferMemoryBarrier::default()
                    .buffer(buffer)
                    .size(vk::WHOLE_SIZE)
                    .src_queue_family_index(hal.queue_family_index())
                    .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ);
                raw.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[release],
                    &[],
                );
                Ok::<_, MediaError>(())
            })?;
        }
        drop(destination);
        drop(hal);
        self.queue.submit([encoder.finish()]);
        self.queue.on_submitted_work_done(move || {
            drop((imported, frame, lease, texture));
        });
        Ok(view)
    }
}
