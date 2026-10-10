use crate::*;
use image::{
    Frame, RgbaImage,
    codecs::gif::{GifEncoder, Repeat},
};
use std::{fs::File, time::Duration};

fn output(blur: f32) -> OutputParams {
    OutputParams {
        width: 32,
        height: 32,
        fit: BackgroundFit::Cover,
        blur,
        limits: PlaybackLimits::default(),
    }
}

fn animation(path: &std::path::Path, count: usize, repeat: Option<Repeat>) {
    let mut encoder = GifEncoder::new(File::create(path).unwrap());
    if let Some(repeat) = repeat {
        encoder.set_repeat(repeat).unwrap();
    }
    for i in 0..count {
        encoder
            .encode_frame(Frame::from_parts(
                RgbaImage::from_pixel(
                    64,
                    64,
                    image::Rgba(if i % 2 == 0 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    }),
                ),
                0,
                0,
                image::Delay::from_numer_denom_ms(if i % 2 == 0 { 100 } else { 200 }, 1),
            ))
            .unwrap();
    }
}

fn next(stream: &mut MediaStream, earliest: Duration, output: OutputParams) -> MediaReply {
    assert!(stream.request_next(earliest, output));
    futures::executor::block_on(stream.recv()).expect("worker response")
}

#[test]
fn animation_controls_use_frame_metadata_instead_of_file_extension() {
    let directory = tempfile::tempdir().unwrap();
    for (count, animated) in [(1, false), (2, true)] {
        let path = directory.path().join(format!("{count}.gif"));
        animation(&path, count, None);
        assert_eq!(is_animated_media(&path).unwrap(), animated);
    }
    for format in [image::ImageFormat::Png, image::ImageFormat::WebP] {
        let path = directory
            .path()
            .join(format!("still.{}", format.extensions_str()[0]));
        RgbaImage::from_pixel(2, 2, image::Rgba([20, 30, 40, 255]))
            .save_with_format(&path, format)
            .unwrap();
        assert!(
            !is_animated_media(&path).unwrap(),
            "{format:?} is a single-frame image"
        );
    }
}

#[test]
fn gallery_posters_share_decode_capacity_instead_of_failing_together() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("gallery.webp");
    RgbaImage::from_pixel(1600, 1000, image::Rgba([31, 62, 93, 255]))
        .save(&path)
        .unwrap();
    let start = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    decode_poster(&path, output(0.0))
                })
            })
            .collect();
        for job in jobs {
            let (_, frame) = job.join().unwrap().expect("every gallery poster loads");
            assert_eq!(&frame.pixels[..4], &[93, 62, 31, 255]);
        }
    });
}

#[test]
fn gif_stream_preserves_pixels_delays_and_finite_loops() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("finite.gif");
    animation(&path, 2, None);
    for blur in [0.0, 4.0] {
        let mut stream = MediaStream::spawn(path.clone(), 7).unwrap();
        for (sequence, color, timestamp, duration) in [
            (1, [0, 0, 255, 255], 0, 100),
            (2, [255, 0, 0, 255], 100, 200),
        ] {
            let MediaReply::Frame { info, frame } = next(&mut stream, Duration::ZERO, output(blur))
            else {
                panic!("missing frame")
            };
            assert_eq!(info.loops, LoopCount::Finite(1));
            assert_eq!((frame.generation, frame.sequence), (7, sequence));
            assert_eq!((frame.width, frame.height), (32, 32));
            assert_eq!(&frame.pixels[..4], &color);
            assert_eq!(frame.timestamp, Duration::from_millis(timestamp));
            assert_eq!(frame.duration, Duration::from_millis(duration));
        }
        assert!(matches!(
            next(&mut stream, Duration::ZERO, output(blur)),
            MediaReply::End
        ));
    }
}

#[test]
fn late_request_advances_composition_and_keeps_media_time() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("loop.gif");
    animation(&path, 2, Some(Repeat::Infinite));
    let mut stream = MediaStream::spawn(path, 1).unwrap();
    let MediaReply::Frame { frame, .. } =
        next(&mut stream, Duration::from_millis(450), output(0.0))
    else {
        panic!("missing frame")
    };
    assert_eq!(frame.sequence, 4);
    assert_eq!(frame.timestamp, Duration::from_millis(400));
    assert_eq!(frame.duration, Duration::from_millis(200));
    assert_eq!(&frame.pixels[..4], &[255, 0, 0, 255]);
}

#[test]
fn one_request_and_one_reply_do_not_decode_the_rest_of_a_long_animation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("long.gif");
    animation(&path, 10_000, None);
    let mut stream = MediaStream::spawn(path, 1).unwrap();
    assert!(stream.request_next(Duration::ZERO, output(0.0)));
    assert!(!stream.request_next(Duration::ZERO, output(0.0)));
    let MediaReply::Frame { frame: first, .. } =
        futures::executor::block_on(stream.recv()).unwrap()
    else {
        panic!("missing frame")
    };
    let budget = stream.memory_budget();
    let initial = budget.reserved_bytes();
    for i in 1..10_000 {
        let MediaReply::Frame { frame, .. } = next(&mut stream, Duration::ZERO, output(0.0)) else {
            panic!("missing frame")
        };
        assert_eq!(frame.sequence, i + 1);
        assert!(budget.reserved_bytes() <= initial + 32 * 32 * 4);
        assert_eq!(
            &first.pixels[..4],
            &[0, 0, 255, 255],
            "retained frames must not be recycled"
        );
        drop(frame);
    }
    drop(first);
    stream.close();
    // The worker observes the channel closure; only its own thread releases codec storage.
    stream.worker.take().unwrap().join().unwrap();
    assert_eq!(budget.reserved_bytes(), 0);
}

#[test]
fn memory_admission_rejects_oversized_canvas_without_allocating() {
    let budget = MemoryBudget::default();
    assert!(matches!(
        budget.reserve(193 * 1024 * 1024),
        Err(MediaError::ResourceExhausted)
    ));
    assert_eq!(budget.reserved_bytes(), 0);
}

#[test]
fn auto_dimensions_and_explicit_limits_never_upscale() {
    let mut params = output(0.0);
    params.width = 1920;
    params.height = 1088;
    for (fit, expected) in [
        (BackgroundFit::Cover, (1920, 1280)),
        (BackgroundFit::Contain, (1632, 1088)),
        (BackgroundFit::Fill, (1920, 1088)),
        (BackgroundFit::Tile, (6000, 4000)),
    ] {
        params.fit = fit;
        assert_eq!(output_dimensions((6000, 4000), params).unwrap(), expected);
        assert_eq!(output_dimensions((60, 40), params).unwrap(), (60, 40));
    }
    params.limits.max_width = Some(1000);
    params.limits.max_height = Some(500);
    assert_eq!(output_dimensions((6000, 4000), params).unwrap(), (750, 500));
}

#[test]
fn native_h264_pixels_timestamps_loops_and_cancellation() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/red-blue.mp4");
    let mut streams = vec![MediaStream::spawn(path.clone(), 42).unwrap()];
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    streams.push(MediaStream::spawn_software(path, 42).unwrap());
    for mut stream in streams.drain(..) {
        let budget = stream.memory_budget();
        let output = OutputParams {
            width: 64,
            height: 48,
            ..output(0.0)
        };
        for (time, expected) in [
            (0, [0, 0, 253, 255]),
            (1000, [254, 0, 0, 255]),
            (2000, [0, 0, 253, 255]),
            (3000, [254, 0, 0, 255]),
        ] {
            let reply = next(&mut stream, Duration::from_millis(time), output);
            let MediaReply::Frame { info, frame } = reply else {
                if let MediaReply::Error(error) = reply {
                    panic!("native decoding failed: {error}");
                }
                panic!("native video ended unexpectedly");
            };
            assert_eq!(info.loops, LoopCount::Infinite);
            assert_eq!((frame.width, frame.height), (64, 48));
            assert_eq!(frame.timestamp, Duration::from_millis(time));
            assert_eq!(frame.duration, Duration::from_millis(40));
            for (&value, expected) in frame.pixels[..4].iter().zip(expected) {
                assert!(
                    i32::from(value).abs_diff(expected) <= 3,
                    "pixel {:?}",
                    &frame.pixels[..4]
                );
            }
        }
        stream.close();
        stream.worker.take().unwrap().join().unwrap();
        assert_eq!(budget.reserved_bytes(), 0);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn native_video_delivery_keeps_the_system_buffer_alive_after_decoder_close() {
    use objc2_core_video::*;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/red-blue.mp4");
    let mut stream = MediaStream::spawn_native(path, 7, NativeVideoDevice::Metal).unwrap();
    let reply = next(
        &mut stream,
        Duration::ZERO,
        OutputParams {
            width: 32,
            height: 24,
            fit: BackgroundFit::Contain,
            blur: 8.0,
            limits: Default::default(),
        },
    );
    let MediaReply::Frame { frame, .. } = reply else {
        panic!("expected native frame")
    };
    assert_eq!(
        (frame.width, frame.height, frame.timestamp),
        (32, 24, Duration::ZERO)
    );
    assert!(
        frame.pixels.is_empty(),
        "native video must not allocate a CPU upload copy"
    );
    let native = frame.native.as_ref().expect("retained system pixel buffer");
    assert_eq!(
        (
            CVPixelBufferGetWidth(&native.buffer),
            CVPixelBufferGetHeight(&native.buffer)
        ),
        (64, 48)
    );
    stream.close();
    stream.worker.take().unwrap().join().unwrap();
    unsafe {
        assert_eq!(
            CVPixelBufferLockBaseAddress(&native.buffer, CVPixelBufferLockFlags::ReadOnly),
            0
        );
        let pixel =
            std::slice::from_raw_parts(CVPixelBufferGetBaseAddress(&native.buffer).cast::<u8>(), 4);
        let actual = [pixel[0], pixel[1], pixel[2], pixel[3]];
        CVPixelBufferUnlockBaseAddress(&native.buffer, CVPixelBufferLockFlags::ReadOnly);
        assert_eq!(actual, [0, 0, 253, 255]);
    }
}

#[test]
fn webp_stream_preserves_composed_colors_delays_and_loop_metadata() {
    const WEBP: &[u8] = &[
        0x52, 0x49, 0x46, 0x46, 0x84, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38,
        0x58, 0x0a, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x41, 0x4e, 0x49, 0x4d, 0x06, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x41,
        0x4e, 0x4d, 0x46, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x56, 0x50, 0x38, 0x4c, 0x0f, 0x00, 0x00,
        0x00, 0x2f, 0x00, 0x00, 0x00, 0x00, 0x07, 0x10, 0xfd, 0x8f, 0xfe, 0x07, 0x22, 0xa2, 0xff,
        0x01, 0x00, 0x41, 0x4e, 0x4d, 0x46, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc8, 0x00, 0x00, 0x00, 0x56, 0x50, 0x38, 0x4c,
        0x0f, 0x00, 0x00, 0x00, 0x2f, 0x00, 0x00, 0x00, 0x00, 0x07, 0x10, 0xd1, 0xff, 0xfe, 0x07,
        0x22, 0xa2, 0xff, 0x01, 0x00,
    ];
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("animation.webp");
    std::fs::write(&path, WEBP).unwrap();
    for blur in [0.0, 4.0] {
        let mut stream = MediaStream::spawn(path.clone(), 1).unwrap();
        for (time, duration, color) in [
            (0, 100, [0, 0, 254, 255]),
            (100, 200, [254, 0, 0, 255]),
            (300, 100, [0, 0, 254, 255]),
        ] {
            let MediaReply::Frame { info, frame } = next(&mut stream, Duration::ZERO, output(blur))
            else {
                panic!("missing WebP frame")
            };
            assert_eq!(info.loops, LoopCount::Infinite);
            assert_eq!(frame.pixels, color);
            assert_eq!(frame.timestamp, Duration::from_millis(time));
            assert_eq!(frame.duration, Duration::from_millis(duration));
        }
    }
}

#[test]
fn gif_partial_transparent_frames_restore_previous_canvas_and_normalize_zero_delay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("patches.gif");
    let mut encoder = gif::Encoder::new(
        File::create(&path).unwrap(),
        3,
        2,
        &[0, 0, 0, 255, 0, 0, 0, 0, 255],
    )
    .unwrap();
    encoder
        .write_frame(&gif::Frame {
            width: 1,
            height: 1,
            buffer: std::borrow::Cow::Borrowed(&[1]),
            delay: 0,
            dispose: gif::DisposalMethod::Keep,
            ..Default::default()
        })
        .unwrap();
    encoder
        .write_frame(&gif::Frame {
            left: 1,
            width: 1,
            height: 1,
            buffer: std::borrow::Cow::Borrowed(&[2]),
            delay: 20,
            dispose: gif::DisposalMethod::Previous,
            ..Default::default()
        })
        .unwrap();
    encoder
        .write_frame(&gif::Frame {
            width: 1,
            height: 1,
            buffer: std::borrow::Cow::Borrowed(&[0]),
            transparent: Some(0),
            delay: 0,
            ..Default::default()
        })
        .unwrap();
    drop(encoder);
    let mut stream = MediaStream::spawn(path, 1).unwrap();
    for (index, expected, millis) in [
        (1, [0, 0, 255, 255, 0, 0, 0, 0], 100),
        (2, [0, 0, 255, 255, 255, 0, 0, 255], 200),
        (3, [0, 0, 255, 255, 0, 0, 0, 0], 100),
    ] {
        let MediaReply::Frame { frame, .. } = next(&mut stream, Duration::ZERO, output(0.0)) else {
            panic!("missing patch")
        };
        assert_eq!(frame.sequence, index);
        assert_eq!(&frame.pixels[..8], &expected);
        assert_eq!(&frame.pixels[8..], &[0; 16]);
        assert_eq!(frame.duration, Duration::from_millis(millis));
    }
}

#[test]
fn consecutive_source_changes_release_completed_worker_budgets() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("switch.gif");
    animation(&path, 2, Some(Repeat::Infinite));
    let mut closed = Vec::new();
    for generation in 0..100 {
        let mut stream = MediaStream::spawn(path.clone(), generation).unwrap();
        let MediaReply::Frame { frame, .. } = next(&mut stream, Duration::ZERO, output(0.0)) else {
            panic!("missing frame")
        };
        assert_eq!(frame.generation, generation);
        drop(frame);
        stream.close();
        closed.push((stream.memory_budget(), stream.worker.take().unwrap()));
    }
    for (budget, worker) in closed {
        worker.join().unwrap();
        assert_eq!(budget.reserved_bytes(), 0);
    }
}

#[test]
fn paused_worker_resumes_and_close_wakes_a_waiting_decoder() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pause.gif");
    animation(&path, 2, Some(Repeat::Infinite));
    let mut stream = MediaStream::spawn(path, 1).unwrap();
    let MediaReply::Frame { frame, .. } = next(&mut stream, Duration::ZERO, output(0.0)) else {
        panic!("missing frame")
    };
    drop(frame);
    let (observer, waiting) = std::sync::mpsc::channel();
    *stream.control.pause_observer.lock().unwrap() = Some(observer);
    stream.pause();
    assert!(stream.request_next(Duration::from_millis(450), output(0.0)));
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(
        stream.receiver.try_recv(),
        Err(async_channel::TryRecvError::Empty)
    ));
    stream.resume();
    let MediaReply::Frame { frame, .. } = futures::executor::block_on(stream.recv()).unwrap()
    else {
        panic!("missing resumed frame")
    };
    assert_eq!(
        (frame.sequence, frame.timestamp),
        (4, Duration::from_millis(400))
    );
    drop(frame);
    stream.pause();
    assert!(stream.request_next(Duration::from_millis(600), output(0.0)));
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let budget = stream.memory_budget();
    stream.close();
    stream.worker.take().unwrap().join().unwrap();
    assert_eq!(budget.reserved_bytes(), 0);
}

#[test]
fn skipped_finite_animation_delivers_its_final_frame_before_end() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("finite.gif");
    animation(&path, 2, Some(Repeat::Finite(1)));
    let mut stream = MediaStream::spawn(path, 1).unwrap();
    let MediaReply::Frame { frame, info } = next(&mut stream, Duration::from_secs(3), output(0.0))
    else {
        panic!("missing final frame")
    };
    assert_eq!(info.loops, LoopCount::Finite(2));
    assert_eq!(frame.sequence, 4);
    assert_eq!(frame.timestamp, Duration::from_millis(400));
    assert_eq!(&frame.pixels[..4], &[255, 0, 0, 255]);
    assert!(matches!(
        next(&mut stream, Duration::from_secs(3), output(0.0)),
        MediaReply::End
    ));
}
