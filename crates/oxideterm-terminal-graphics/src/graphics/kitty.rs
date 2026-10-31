// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, PartialEq, Eq)]
enum KittyAction {
    Transmit,
    Display,
    Query,
    Place,
    Delete,
    Frame,
    Animate,
    Compose,
}

#[cfg(test)]
mod kitty_tests {
    use super::*;

    fn cursor() -> GraphicsCursor {
        GraphicsCursor {
            line: 0,
            row: 0,
            col: 0,
            cols: 80,
            rows: 24,
            cell_width: 10,
            cell_height: 20,
        }
    }

    #[test]
    fn queries_validate_without_replacing_pixels_and_obey_quiet_levels() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        ingress.advance(b"\x1b_Ga=t,i=5,f=32,s=1,v=1;/wAA/w==\x1b\\", cursor());
        let valid = ingress.advance(b"\x1b_Ga=q,i=5,f=24,s=1,v=1;AP8A\x1b\\", cursor());
        assert_eq!(
            valid.events,
            [TerminalGraphicsEvent::Respond(
                b"\x1b_Gi=5;OK\x1b\\".to_vec()
            )]
        );
        assert_eq!(
            ingress.kitty_images[&TerminalImageId(5)].rgba.as_ref(),
            &[255, 0, 0, 255]
        );
        for (quiet, valid, reply) in [
            (0, false, Some("EINVAL:Invalid graphics command or data")),
            (1, false, Some("EINVAL:Invalid graphics command or data")),
            (1, true, None),
            (2, false, None),
            (2, true, None),
        ] {
            let payload = if valid { "AP8A" } else { "AA==" };
            let bytes = format!("\x1b_Ga=q,i=6,f=24,s=1,v=1,q={quiet};{payload}\x1b\\");
            let result = ingress.advance(bytes.as_bytes(), cursor());
            let responses = result
                .events
                .into_iter()
                .filter_map(|e| match e {
                    TerminalGraphicsEvent::Respond(r) => Some(r),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                responses,
                reply
                    .map(|s| format!("\x1b_Gi=6;{s}\x1b\\").into_bytes())
                    .into_iter()
                    .collect::<Vec<_>>()
            );
        }
        assert!(!ingress.kitty_images.contains_key(&TerminalImageId(6)));
        for command in [
            b"\x1b_Ga=q,i=6,f=32,s=1,v=1,broken;/wAA/w==\x1b\\".as_slice(),
            b"\x1b_Ga=q,i=6,I=7,f=32,s=1,v=1;/wAA/w==\x1b\\",
        ] {
            let result = ingress.advance(command, cursor());
            assert!(result.events.contains(&TerminalGraphicsEvent::Respond(
                if command.windows(3).any(|w| w == b"I=7") {
                    b"\x1b_Gi=6,I=7;EINVAL:Invalid graphics command or data\x1b\\".to_vec()
                } else {
                    b"\x1b_Gi=6;EINVAL:Invalid graphics command or data\x1b\\".to_vec()
                }
            )));
        }
        ingress.advance(b"\x1b_Ga=t,i=8,f=32,s=1,v=1,m=1;AP8A\x1b\\", cursor());
        let invalid = ingress.advance(b"\x1b_Gm=bad;/w==\x1b\\", cursor());
        assert_eq!(
            invalid.events,
            [TerminalGraphicsEvent::Respond(
                b"\x1b_Gi=8;EINVAL:Invalid graphics command or data\x1b\\".to_vec()
            )]
        );
        let absent = ingress.advance(b"\x1b_Ga=p,i=8\x1b\\", cursor());
        assert!(absent.events.contains(&TerminalGraphicsEvent::Respond(
            b"\x1b_Gi=8;ENOENT:Image or placement not found\x1b\\".to_vec()
        )));
    }

    #[test]
    fn numbers_select_newest_image_and_placements_have_independent_lifetimes() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        for (payload, id) in [("/wAA/w==", 1), ("AP8A/w==", 2)] {
            let result = ingress.advance(
                format!("\x1b_Ga=t,I=7,f=32,s=1,v=1;{payload}\x1b\\").as_bytes(),
                cursor(),
            );
            assert!(result.events.contains(&TerminalGraphicsEvent::Respond(
                format!("\x1b_Gi={id},I=7;OK\x1b\\").into_bytes()
            )));
        }
        for placement in [0, 0, 8, 8] {
            ingress.advance(
                format!("\x1b_Ga=p,I=7,p={placement},C=1\x1b\\").as_bytes(),
                cursor(),
            );
        }
        assert_eq!(
            ingress
                .kitty_placements
                .iter()
                .map(|p| (p.id.0, p.kitty.as_ref().unwrap().id))
                .collect::<Vec<_>>(),
            [(2, 0), (2, 0), (2, 8)]
        );
        let deletion = ingress.advance(b"\x1b_Ga=d,d=n,I=7,p=8,q=2\x1b\\", cursor());
        assert_eq!(
            deletion.events,
            [TerminalGraphicsEvent::DeletePlacement { serial: 4 }]
        );
        assert_eq!(ingress.kitty_placements.len(), 2);
        assert_eq!(
            ingress.kitty_images[&TerminalImageId(1)].rgba.as_ref(),
            &[255, 0, 0, 255]
        );
        assert_eq!(
            ingress.kitty_images[&TerminalImageId(2)].rgba.as_ref(),
            &[0, 255, 0, 255]
        );
    }

    #[test]
    fn deletion_selectors_remove_only_matching_placements() {
        for (selector, expected) in [
            ("d=p,x=4,y=3", vec![2]),
            ("d=x,x=1", vec![1]),
            ("d=y,y=3", vec![2]),
            ("d=z,z=-2", vec![3]),
            ("d=r,x=2,y=3", vec![2, 3]),
            ("d=q,x=4,y=3,z=-1", vec![2]),
            ("d=c", vec![1]),
        ] {
            let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
            for (id, col, line, z) in [(1, 0, 0, 0), (2, 3, 2, -1), (3, 6, 4, -2)] {
                let mut at = cursor();
                at.col = col;
                at.line = line;
                at.row = line as usize;
                ingress.advance(
                    format!("\x1b_Ga=T,i={id},p=1,f=32,s=1,v=1,C=1,z={z},q=2;/wAA/w==\x1b\\")
                        .as_bytes(),
                    at,
                );
            }
            let result = ingress.advance(
                format!("\x1b_Ga=d,{selector},q=2\x1b\\").as_bytes(),
                cursor(),
            );
            let deleted = result
                .events
                .into_iter()
                .filter_map(|event| match event {
                    TerminalGraphicsEvent::DeletePlacement { serial } => Some(serial),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(deleted, expected, "{selector}");
            assert_eq!(ingress.kitty_images.len(), 3, "soft delete retained data");
        }
    }

    #[test]
    fn compressed_pixels_are_bounded_and_file_slices_follow_symlinks() {
        use std::io::Write;
        let mut compressed =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        compressed.write_all(&[9, 8, 7, 255]).unwrap();
        let payload = BASE64.encode(compressed.finish().unwrap());
        let options = GraphicsOptions {
            storage_limit_mb: 1,
            ..GraphicsOptions::default()
        };
        let control = options.kitty_file_transmission.clone();
        let mut ingress = GraphicsIngress::new(options);
        let result = ingress.advance(
            format!("\x1b_Ga=t,i=8,f=32,s=1,v=1,o=z;{payload}\x1b\\").as_bytes(),
            cursor(),
        );
        let image = result
            .events
            .iter()
            .find_map(|e| match e {
                TerminalGraphicsEvent::ImageReady(i) => Some(i),
                _ => None,
            })
            .unwrap();
        assert_eq!(image.rgba.as_ref(), &[9, 8, 7, 255]);
        let mut compressed =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        compressed.write_all(&vec![0; 1024 * 1024 + 1]).unwrap();
        let payload = BASE64.encode(compressed.finish().unwrap());
        let result = ingress.advance(
            format!("\x1b_Ga=t,i=9,f=32,s=1,v=1,o=z;{payload}\x1b\\").as_bytes(),
            cursor(),
        );
        assert!(result.events.contains(&TerminalGraphicsEvent::Respond(
            b"\x1b_Gi=9;ENOSPC:Graphics resource limit exceeded\x1b\\".to_vec()
        )));
        control.authorize_for_session().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image");
        fs::write(&path, [1, 2, 9, 8, 7, 255, 3, 4]).unwrap();
        #[cfg(unix)]
        let path = {
            let link = dir.path().join("link");
            std::os::unix::fs::symlink(path, &link).unwrap();
            link
        };
        let result = ingress.advance(
            format!(
                "\x1b_Ga=t,i=10,f=32,s=1,v=1,t=f,O=2,S=4;{}\x1b\\",
                BASE64.encode(path.to_str().unwrap())
            )
            .as_bytes(),
            cursor(),
        );
        let image = result
            .events
            .iter()
            .find_map(|e| match e {
                TerminalGraphicsEvent::ImageReady(i) => Some(i),
                _ => None,
            })
            .unwrap();
        assert_eq!(image.rgba.as_ref(), &[9, 8, 7, 255]);
    }

    #[test]
    fn frame_composition_uses_source_uppercase_and_preserves_loop_controls() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let root = BASE64.encode([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
        ingress.advance(
            format!("\x1b_Ga=t,i=22,f=32,s=3,v=1,q=2;{root}\x1b\\").as_bytes(),
            cursor(),
        );
        ingress.advance(
            b"\x1b_Ga=f,i=22,f=32,s=1,v=1,c=1,x=0,y=0,q=2;//8A/w==\x1b\\",
            cursor(),
        );
        let composed = ingress.advance(
            b"\x1b_Ga=c,i=22,r=2,c=1,X=1,Y=0,x=2,y=0,w=1,h=1,C=1,q=2\x1b\\",
            cursor(),
        );
        let image = composed
            .events
            .iter()
            .find_map(|e| match e {
                TerminalGraphicsEvent::ImageUpdated(i) => Some(i),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            image.rgba.as_ref(),
            &[255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255]
        );
        for command in [
            b"\x1b_Ga=a,i=22,s=3,v=2,q=2\x1b\\".as_slice(),
            b"\x1b_Ga=a,i=22,s=2,q=2\x1b\\",
            b"\x1b_Ga=a,i=22,s=3,v=0,q=2\x1b\\",
        ] {
            let result = ingress.advance(command, cursor());
            let image = result
                .events
                .iter()
                .find_map(|e| match e {
                    TerminalGraphicsEvent::ImageUpdated(i) => Some(i),
                    _ => None,
                })
                .unwrap();
            assert_eq!(image.animation.loop_limit, Some(1));
        }
        let before = ingress.kitty_images[&TerminalImageId(22)].clone();
        let invalid = ingress.advance(b"\x1b_Ga=a,i=22,s=1,c=99\x1b\\", cursor());
        assert!(invalid.events.contains(&TerminalGraphicsEvent::Respond(
            b"\x1b_Gi=22;EINVAL:Invalid graphics command or data\x1b\\".to_vec()
        )));
        assert_eq!(ingress.kitty_images[&TerminalImageId(22)], before);
        ingress.advance(b"\x1b_Ga=d,d=f,i=22,r=1,q=2\x1b\\", cursor());
        assert_eq!(
            ingress.kitty_images[&TerminalImageId(22)].rgba.as_ref(),
            &[255, 255, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]
        );
        ingress.advance(b"\x1b_Ga=d,d=f,i=22,q=2\x1b\\", cursor());
        assert_eq!(ingress.kitty_images[&TerminalImageId(22)].frames.len(), 1);
        ingress.advance(b"\x1b_Ga=d,d=F,i=22,q=2\x1b\\", cursor());
        assert!(!ingress.kitty_images.contains_key(&TerminalImageId(22)));
        ingress.advance(b"\x1b_Ga=t,i=23,f=32,s=1,v=1,q=2;/wAA/w==\x1b\\", cursor());
        ingress.advance(
            b"\x1b_Ga=f,i=23,c=1,f=32,s=1,v=1,q=2;AAD/gA==\x1b\\",
            cursor(),
        );
        assert_eq!(
            ingress.kitty_images[&TerminalImageId(23)].frames[1]
                .rgba
                .as_ref(),
            &[127, 0, 128, 255]
        );
        ingress.advance(b"\x1b_Ga=f,i=23,f=32,s=1,v=1,q=2;AAD/gA==\x1b\\", cursor());
        ingress.advance(b"\x1b_Ga=c,i=23,r=3,c=1,C=1,q=2\x1b\\", cursor());
        assert_eq!(
            ingress.kitty_images[&TerminalImageId(23)].rgba.as_ref(),
            &[0, 0, 255, 128]
        );
    }

    #[cfg(unix)]
    #[test]
    fn shared_memory_requires_authorization_and_is_unlinked_after_read() {
        use std::os::fd::{AsRawFd, FromRawFd};
        let name = format!("/oxideterm-kgp-{}", std::process::id());
        let c_name = std::ffi::CString::new(name.clone()).unwrap();
        let fd = unsafe {
            libc::shm_open(
                c_name.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o600,
            )
        };
        assert!(fd >= 0);
        let file = unsafe { fs::File::from_raw_fd(fd) };
        file.set_len(4).unwrap();
        let address = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        assert_ne!(address, libc::MAP_FAILED);
        unsafe {
            std::ptr::copy_nonoverlapping([4u8, 3, 2, 255].as_ptr(), address.cast(), 4);
            libc::munmap(address, 4);
        }
        let options = GraphicsOptions::default();
        let control = options.kitty_file_transmission.clone();
        let mut ingress = GraphicsIngress::new(options);
        let command = format!(
            "\x1b_Ga=t,i=11,f=32,s=1,v=1,t=s,S=4;{}\x1b\\",
            BASE64.encode(name)
        );
        let denied = ingress.advance(command.as_bytes(), cursor());
        assert!(denied.events.contains(&TerminalGraphicsEvent::Respond(
            b"\x1b_Gi=11;EBADF:Failed to read image file\x1b\\".to_vec()
        )));
        control.authorize_for_session().unwrap();
        let result = ingress.advance(command.as_bytes(), cursor());
        let image = result
            .events
            .iter()
            .find_map(|e| match e {
                TerminalGraphicsEvent::ImageReady(i) => Some(i),
                _ => None,
            })
            .unwrap();
        assert_eq!(image.rgba.as_ref(), &[4, 3, 2, 255]);
        assert_eq!(
            unsafe { libc::shm_open(c_name.as_ptr(), libc::O_RDONLY, 0) },
            -1
        );
    }
}

#[cfg(test)]
mod relative_tests {
    use super::*;

    #[test]
    fn relative_chains_follow_named_replacement_and_reject_invalid_parents() {
        let cursor = GraphicsCursor {
            line: 2,
            row: 2,
            col: 4,
            cols: 80,
            rows: 24,
            cell_width: 10,
            cell_height: 20,
        };
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        ingress.advance(
            b"\x1b_Ga=T,i=2,p=1,C=1,f=32,s=1,v=1,q=2;/wAA/w==\x1b\\",
            cursor,
        );
        for p in 2..=9 {
            let result = ingress.advance(
                format!("\x1b_Ga=p,i=2,p={p},P=2,Q={},H=-1,V=1\x1b\\", p - 1).as_bytes(),
                cursor,
            );
            assert!(result.events.contains(&TerminalGraphicsEvent::Respond(
                format!("\x1b_Gi=2,p={p};OK\x1b\\").into_bytes()
            )));
            assert!(
                !result
                    .events
                    .iter()
                    .any(|e| matches!(e, TerminalGraphicsEvent::MoveCursor { .. }))
            );
        }
        let last = ingress.kitty_placements.last().unwrap();
        assert_eq!(
            (
                last.line,
                last.col,
                last.kitty.as_ref().unwrap().column_offset
            ),
            (10, 0, -4)
        );
        for (placement, parent, error) in
            [(10, 9, "ETOODEEP"), (1, 9, "ECYCLE"), (10, 99, "ENOPARENT")]
        {
            let result = ingress.advance(
                format!("\x1b_Ga=p,i=2,p={placement},P=2,Q={parent}\x1b\\").as_bytes(),
                cursor,
            );
            let reply = result
                .events
                .iter()
                .find_map(|e| match e {
                    TerminalGraphicsEvent::Respond(r) => Some(String::from_utf8_lossy(r)),
                    _ => None,
                })
                .unwrap();
            assert!(reply.contains(error), "{reply}");
        }
        ingress.advance(
            b"\x1b_Ga=p,i=2,p=1,C=1,q=2\x1b\\",
            GraphicsCursor {
                line: 0,
                col: 10,
                ..cursor
            },
        );
        let child = ingress
            .kitty_placements
            .iter()
            .find(|p| p.kitty.as_ref().unwrap().id == 9)
            .unwrap();
        assert_eq!((child.line, child.col), (8, 2));
        ingress.advance(b"\x1b_Ga=d,d=i,i=2,p=1,q=2\x1b\\", cursor);
        assert!(ingress.kitty_placements.is_empty());
        assert!(!ingress.kitty_images.contains_key(&TerminalImageId(2)));
    }
}

struct KittyCommand {
    action: KittyAction,
    params: HashMap<String, String>,
}

impl KittyCommand {
    fn new(mut params: HashMap<String, String>) -> Result<Self, GraphicsError> {
        let action = match params.get("a").map(String::as_str).unwrap_or("t") {
            "t" => KittyAction::Transmit,
            "T" => KittyAction::Display,
            "q" => KittyAction::Query,
            "p" => KittyAction::Place,
            "d" => KittyAction::Delete,
            "f" => KittyAction::Frame,
            "a" => KittyAction::Animate,
            "c" => KittyAction::Compose,
            _ => return Err(GraphicsError::InvalidCommand),
        };
        if params.contains_key("i") && params.contains_key("I") {
            return Err(GraphicsError::InvalidCommand);
        }
        for (key, value) in &mut params {
            match key.as_str() {
                "z" | "H" | "V" => {
                    *value = value
                        .parse::<i32>()
                        .map_err(|_| GraphicsError::InvalidCommand)?
                        .to_string();
                }
                "i" | "I" | "p" | "s" | "v" | "x" | "y" | "w" | "h" | "X" | "Y" | "c" | "r"
                | "C" | "U" | "P" | "Q" | "S" | "O" | "f" | "m" | "q" | "N" => {
                    *value = value
                        .parse::<u32>()
                        .map_err(|_| GraphicsError::InvalidCommand)?
                        .to_string();
                }
                _ => {}
            }
        }
        if parse_u32(params.get("q")).is_some_and(|q| q > 2)
            || parse_u32(params.get("m")).is_some_and(|m| m > 1)
            || parse_u32(params.get("C")).is_some_and(|c| c > 1)
            || parse_u32(params.get("U")).is_some_and(|u| u > 1)
        {
            return Err(GraphicsError::InvalidCommand);
        }
        Ok(Self { action, params })
    }
}

impl GraphicsIngress {
    fn dispatch_kitty(
        &mut self,
        data: &[u8],
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) {
        // Control headers are tiny; bound allocation independently of the pixel payload.
        if data
            .iter()
            .position(|byte| *byte == b';')
            .unwrap_or(data.len())
            > 4096
        {
            self.kitty_chunks.clear();
            result.events.push(TerminalGraphicsEvent::Error(
                GraphicsError::InvalidCommand.to_string(),
            ));
            return;
        }
        let (mut params, payload) = parse_kitty_command(data);
        let header = data.split(|byte| *byte == b';').next().unwrap_or_default();
        if !header.is_empty()
            && header.split(|byte| *byte == b',').any(|part| {
                part.len() < 3
                    || !part[0].is_ascii_alphabetic()
                    || part[1] != b'='
                    || !part[2..].iter().all(u8::is_ascii_graphic)
            })
        {
            self.kitty_chunks.clear();
            self.kitty_reply(&params, Err(&GraphicsError::InvalidCommand), result);
            return;
        }
        let transmission = matches!(
            params.get("a").map(String::as_str).unwrap_or("t"),
            "t" | "T" | "q" | "f"
        );
        let mut encoded = Vec::new();
        if transmission {
            let previous = self.kitty_chunks.remove(&0);
            if let Some(previous) = &previous {
                let frame = previous.params.get("a").is_some_and(|a| a == "f");
                if params.keys().any(|key| {
                    !matches!(key.as_str(), "m" | "q")
                        && !(frame && key == "a" && params[key] == "f")
                }) || params
                    .get("m")
                    .is_some_and(|m| m.parse::<u32>().map_or(true, |m| m > 1))
                    || params
                        .get("q")
                        .is_some_and(|q| q.parse::<u32>().map_or(true, |q| q > 2))
                {
                    self.kitty_reply(
                        &previous.params,
                        Err(&GraphicsError::InvalidCommand),
                        result,
                    );
                    return;
                }
            }
            let mut assembly = previous.unwrap_or_else(|| KittyChunkAssembly {
                params: params.clone(),
                encoded: Vec::new(),
            });
            if let Some(quiet) = params.get("q") {
                assembly.params.insert("q".into(), quiet.clone());
            }
            let payload = payload.unwrap_or_default();
            if assembly.encoded.len().saturating_add(payload.len()) > encoded_storage_limit() {
                self.kitty_reply(
                    &assembly.params,
                    Err(&GraphicsError::StorageLimitExceeded),
                    result,
                );
                result.events.push(TerminalGraphicsEvent::Error(
                    GraphicsError::StorageLimitExceeded.to_string(),
                ));
                return;
            }
            assembly.encoded.extend_from_slice(payload);
            if parse_u32(params.get("m")) == Some(1) {
                self.kitty_chunks.insert(0, assembly);
                return;
            }
            params = assembly.params;
            encoded = assembly.encoded;
        }
        let command = match KittyCommand::new(params.clone()) {
            Ok(command) => command,
            Err(error) => {
                self.kitty_reply(&params, Err(&error), result);
                result
                    .events
                    .push(TerminalGraphicsEvent::Error(match error {
                        GraphicsError::Decode(_) => GraphicsError::InvalidCommand.to_string(),
                        _ => error.to_string(),
                    }));
                return;
            }
        };
        let outcome = self.execute_kitty(command, &encoded, cursor, result);
        if matches!(
            params.get("a").map(String::as_str).unwrap_or("t"),
            "t" | "T" | "p" | "d"
        ) {
            self.placeholders_changed = true;
        }
        self.refresh_placeholders(&mut result.events);
        self.cascade_missing_parents(&mut result.events);
        self.refresh_relatives(&mut result.events);
        match outcome {
            Ok(reply_params) => self.kitty_reply(&reply_params, Ok(()), result),
            Err(error) => {
                self.kitty_reply(&params, Err(&error), result);
                result
                    .events
                    .push(TerminalGraphicsEvent::Error(match error {
                        GraphicsError::Decode(_) => GraphicsError::InvalidCommand.to_string(),
                        _ => error.to_string(),
                    }));
            }
        }
    }

    fn kitty_reply(
        &self,
        params: &HashMap<String, String>,
        outcome: Result<(), &GraphicsError>,
        result: &mut GraphicsAdvance,
    ) {
        let quiet = parse_u32(params.get("q")).unwrap_or(0);
        if quiet == 2 || (quiet == 1 && outcome.is_ok()) {
            return;
        }
        let id = parse_u32(params.get("i")).unwrap_or(0);
        let number = parse_u32(params.get("I")).unwrap_or(0);
        if id == 0 && number == 0 && params.get("a").is_none_or(|a| a != "q") {
            return;
        }
        let mut response = format!("\x1b_Gi={id}");
        if number != 0 {
            response.push_str(&format!(",I={number}"));
        }
        if let Some(placement) = parse_u32(params.get("p")).filter(|p| *p != 0) {
            response.push_str(&format!(",p={placement}"));
        }
        let message = match outcome {
            Ok(()) => "OK",
            Err(GraphicsError::ImageNotFound) => "ENOENT:Image or placement not found",
            Err(GraphicsError::ParentNotFound) => "ENOPARENT:Parent placement not found",
            Err(GraphicsError::PlacementCycle) => "ECYCLE:Relative placement cycle",
            Err(GraphicsError::PlacementDepth) => "ETOODEEP:Relative placement depth exceeded",
            Err(
                GraphicsError::LocalFileTransmissionDisabled
                | GraphicsError::InvalidLocalFileTransmissionPath
                | GraphicsError::LocalFileTransmissionAccessFailed,
            ) => "EBADF:Failed to read image file",
            Err(GraphicsError::StorageLimitExceeded | GraphicsError::PixelLimitExceeded) => {
                "ENOSPC:Graphics resource limit exceeded"
            }
            Err(_) => "EINVAL:Invalid graphics command or data",
        };
        response.push(';');
        response.push_str(message);
        response.push_str("\x1b\\");
        result
            .events
            .push(TerminalGraphicsEvent::Respond(response.into_bytes()));
    }

    fn kitty_resource(
        &self,
        params: &HashMap<String, String>,
    ) -> Result<TerminalImageId, GraphicsError> {
        if let Some(id) = parse_u32(params.get("i")).filter(|id| *id != 0) {
            let id = self
                .kitty_wire_ids
                .get(&(self.alternate_screen, id))
                .copied()
                .ok_or(GraphicsError::ImageNotFound)?;
            return self
                .kitty_images
                .contains_key(&id)
                .then_some(id)
                .ok_or(GraphicsError::ImageNotFound);
        }
        if let Some(number) = parse_u32(params.get("I")).filter(|id| *id != 0) {
            return self
                .kitty_numbers
                .get(&(self.alternate_screen, number))
                .and_then(|ids| {
                    ids.iter()
                        .rev()
                        .find(|id| self.kitty_images.contains_key(id))
                })
                .copied()
                .ok_or(GraphicsError::ImageNotFound);
        }
        Err(GraphicsError::ImageNotFound)
    }

    fn execute_kitty(
        &mut self,
        command: KittyCommand,
        encoded: &[u8],
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) -> Result<HashMap<String, String>, GraphicsError> {
        let KittyCommand { action, mut params } = command;
        match action {
            KittyAction::Delete => {
                let wire = self.kitty_resource(&params).ok().and_then(|id| {
                    self.kitty_wire_ids
                        .iter()
                        .find_map(|((_, wire), resource)| (*resource == id).then_some(*wire))
                });
                self.kitty_chunks.clear();
                self.delete_kitty(&params, cursor, result)?;
                if let Some(wire) = wire {
                    params.insert("i".into(), wire.to_string());
                }
            }
            KittyAction::Place
            | KittyAction::Animate
            | KittyAction::Compose
            | KittyAction::Frame => {
                let mut reply = params.clone();
                let id = self.kitty_resource(&params)?;
                if let Some(wire) = self
                    .kitty_wire_ids
                    .iter()
                    .find_map(|((_, wire), resource)| (*resource == id).then_some(*wire))
                {
                    reply.insert("i".into(), wire.to_string());
                }
                params.insert("i".into(), id.0.to_string());
                match action {
                    KittyAction::Place => self.put_kitty(&params, cursor, result)?,
                    KittyAction::Animate => result.events.push(
                        TerminalGraphicsEvent::ImageUpdated(self.control_kitty_animation(&params)?),
                    ),
                    KittyAction::Compose => {
                        result.events.push(TerminalGraphicsEvent::ImageUpdated(
                            self.compose_kitty_animation_frame(&params)?,
                        ))
                    }
                    KittyAction::Frame => {
                        let decoded = self.decode_kitty_pixels(&params, encoded)?;
                        result.events.push(TerminalGraphicsEvent::ImageUpdated(
                            self.add_kitty_animation_frame(&params, decoded)?,
                        ));
                    }
                    _ => unreachable!(),
                }
                return Ok(reply);
            }
            KittyAction::Query | KittyAction::Transmit | KittyAction::Display => {
                let decoded = self.decode_kitty_pixels(&params, encoded)?;
                if action == KittyAction::Query {
                    return Ok(params);
                }
                let wire_id = if let Some(id) = parse_u32(params.get("i")).filter(|id| *id != 0) {
                    id
                } else if parse_u32(params.get("I")).is_some_and(|i| i > 0) {
                    let id = (1..=u32::MAX)
                        .find(|id| {
                            !self
                                .kitty_wire_ids
                                .contains_key(&(self.alternate_screen, *id))
                        })
                        .ok_or(GraphicsError::StorageLimitExceeded)?;
                    params.insert("i".into(), id.to_string());
                    id
                } else {
                    0
                };
                let id = if wire_id == 0 {
                    self.next_id()
                } else if let Some(id) = self.kitty_wire_ids.get(&(self.alternate_screen, wire_id))
                {
                    *id
                } else if self.alternate_screen {
                    self.next_id()
                } else {
                    TerminalImageId(u64::from(wire_id))
                };
                if wire_id != 0 {
                    self.kitty_wire_ids
                        .insert((self.alternate_screen, wire_id), id);
                }
                let image = terminal_image_data_from_decoded(
                    id,
                    TerminalImageProtocol::Kitty,
                    decoded,
                    None,
                );
                if let Some(number) = parse_u32(params.get("I")).filter(|number| *number > 0) {
                    self.kitty_numbers
                        .entry((self.alternate_screen, number))
                        .or_default()
                        .push(id);
                }
                self.kitty_placements.retain(|p| {
                    if p.id == id {
                        result.events.push(TerminalGraphicsEvent::DeletePlacement {
                            serial: p.kitty.as_ref().unwrap().serial,
                        });
                    }
                    p.id != id
                });
                self.cascade_missing_parents(&mut result.events);
                self.kitty_images.insert(id, image.clone());
                self.placeholder_rendered.retain(|_, p| p.id != id);
                result.events.push(TerminalGraphicsEvent::ImageReady(image));
                if action == KittyAction::Display {
                    let mut placement_params = params.clone();
                    placement_params.insert("i".into(), id.0.to_string());
                    self.put_kitty(&placement_params, cursor, result)?;
                }
            }
        }
        Ok(params)
    }

    fn decode_kitty_pixels(
        &self,
        params: &HashMap<String, String>,
        encoded: &[u8],
    ) -> Result<DecodedPixels, GraphicsError> {
        let complete = decode_kitty_payload(
            params,
            encoded,
            MAX_TRANSFER_MB,
            &self.options.kitty_file_transmission,
        )?;
        match params.get("f").map(String::as_str).unwrap_or("32") {
            "24" | "32" => {
                let (width, height) = raw_dimensions(params)?;
                decoded_rgba_bytes(
                    width,
                    height,
                    self.options.pixel_limit,
                    self.options.storage_limit_mb,
                )?;
                if params.get("f").is_some_and(|f| f == "24") {
                    decode_raw_rgb(&complete, params)
                } else {
                    decode_raw_rgba(&complete, params)
                }
            }
            "100" => {
                if image::guess_format(&complete).ok() != Some(image::ImageFormat::Png) {
                    return Err(GraphicsError::UnsupportedImage);
                }
                decode_image_bytes(
                    &complete,
                    self.options.pixel_limit,
                    self.options.storage_limit_mb,
                )
            }
            _ => Err(GraphicsError::UnsupportedImage),
        }
    }

    fn put_kitty(
        &mut self,
        params: &HashMap<String, String>,
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) -> Result<(), GraphicsError> {
        let metadata = self
            .kitty_placements
            .len()
            .saturating_add(1)
            .saturating_mul(std::mem::size_of::<TerminalImagePlacement>() * 2);
        enforce_storage_limit(metadata, MAX_GRAPHICS_METADATA_MB)?;
        let (mut placement, _) = self.place_kitty_image(params, cursor)?;
        let named_image = self.kitty_wire_ids.values().any(|id| *id == placement.id);
        let wire_id = if named_image {
            parse_u32(params.get("p")).unwrap_or(0)
        } else {
            0
        };
        let parent = self.kitty_parent(placement.id, wire_id, params)?;
        if wire_id != 0 {
            let replaced = self
                .kitty_placements
                .iter()
                .filter(|p| {
                    p.id == placement.id && p.kitty.as_ref().is_some_and(|k| k.id == wire_id)
                })
                .filter_map(|p| p.kitty.as_ref().map(|k| k.serial))
                .collect::<Vec<_>>();
            for serial in replaced {
                self.kitty_placements
                    .retain(|p| p.kitty.as_ref().is_none_or(|k| k.serial != serial));
                result
                    .events
                    .push(TerminalGraphicsEvent::DeletePlacement { serial });
            }
        }
        placement.kitty = Some(KittyPlacement {
            serial: self.next_placement_serial,
            id: wire_id,
            offset_x: parse_u32(params.get("X")).unwrap_or(0),
            offset_y: parse_u32(params.get("Y")).unwrap_or(0),
            scaled: parse_positive_usize(params.get("c")).is_some()
                || parse_positive_usize(params.get("r")).is_some(),
            virtual_placement: params.get("U").is_some_and(|u| u == "1"),
            parent,
            clip_top: 0,
            clip_bottom: 0,
            image_id: self
                .kitty_wire_ids
                .iter()
                .find_map(|((_, id), resource)| (*resource == placement.id).then_some(*id))
                .unwrap_or(0),
            column_offset: 0,
            hidden: false,
            unicode: None,
        });
        self.resolve_placement(&mut placement);
        self.next_placement_serial += 1;
        if self.placeholder_index_incomplete && params.get("U").is_some_and(|u| u == "1") {
            if let Some(grid) = &self.grid_events {
                grid.invalidate_placeholders();
            }
            self.placeholder_index_incomplete = false;
        }
        self.kitty_placements.push(placement.clone());
        if let Some(grid) = &self.grid_events
            && placement
                .kitty
                .as_ref()
                .is_some_and(|k| k.parent.is_none() && !k.virtual_placement)
        {
            grid.set_anchor(
                self.alternate_screen,
                self.next_placement_serial - 1,
                Some(alacritty_terminal::index::Point::new(
                    alacritty_terminal::index::Line(placement.line),
                    alacritty_terminal::index::Column(placement.col),
                )),
            );
        }
        if !params.get("U").is_some_and(|u| u == "1") {
            if parent.is_none() && !params.get("C").is_some_and(|c| c == "1") {
                result
                    .events
                    .push(TerminalGraphicsEvent::Place(placement.clone()));
                result.events.push(TerminalGraphicsEvent::MoveCursor {
                    cols: placement.cols,
                    rows: placement.rows,
                });
                return Ok(());
            }
        }
        result.events.push(TerminalGraphicsEvent::Place(placement));
        Ok(())
    }

    fn delete_kitty(
        &mut self,
        params: &HashMap<String, String>,
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) -> Result<(), GraphicsError> {
        let selector = params.get("d").map(String::as_str).unwrap_or("a");
        let free_data = selector
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_uppercase());
        let selector = selector.to_ascii_lowercase();
        let image = self.kitty_resource(params).ok();
        let range_ids = self
            .kitty_wire_ids
            .iter()
            .filter_map(|((screen, wire), resource)| {
                (*screen == self.alternate_screen
                    && *wire >= parse_u32(params.get("x")).unwrap_or(0)
                    && *wire <= parse_u32(params.get("y")).unwrap_or(u32::MAX))
                .then_some(*resource)
            })
            .collect::<std::collections::HashSet<_>>();
        let point_x = parse_u32(params.get("x")).unwrap_or(1).saturating_sub(1) as usize;
        let point_y = i64::from(parse_u32(params.get("y")).unwrap_or(1).saturating_sub(1));
        let z = params
            .get("z")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        if !matches!(
            selector.as_str(),
            "a" | "i" | "n" | "c" | "p" | "q" | "r" | "x" | "y" | "z" | "f"
        ) {
            return Err(GraphicsError::InvalidCommand);
        }
        if selector == "f" {
            return self.delete_kitty_frame(params, result);
        }
        let mut affected = Vec::new();
        let mut removed = Vec::new();
        self.kitty_placements.retain(|placement| {
            if placement.alternate_screen != self.alternate_screen {
                return true;
            }
            let kitty = placement.kitty.as_ref().expect("Kitty placement metadata");
            let left = placement.col as i64 + kitty.column_offset;
            let top = i64::from(placement.line) + kitty.clip_top as i64;
            let bottom =
                i64::from(placement.line) + placement.rows as i64 - kitty.clip_bottom as i64;
            let contains = |x: usize, y: i64| {
                left <= x as i64
                    && (x as i64) < left.saturating_add(placement.cols as i64)
                    && top <= y
                    && y < bottom
            };
            let selected = if (kitty.virtual_placement
                && !matches!(selector.as_str(), "i" | "n" | "r"))
                || (kitty.hidden && !matches!(selector.as_str(), "i" | "n" | "r" | "z"))
            {
                false
            } else {
                match selector.as_str() {
                    "a" => top < cursor.rows as i64 && bottom > 0,
                    "i" | "n" => {
                        image == Some(placement.id)
                            && parse_u32(params.get("p")).is_none_or(|p| p == 0 || p == kitty.id)
                    }
                    "c" => contains(cursor.col, i64::from(cursor.line)),
                    "p" => contains(point_x, point_y),
                    "q" => contains(point_x, point_y) && placement.z_index == z,
                    "x" => {
                        left <= point_x as i64
                            && (point_x as i64) < left.saturating_add(placement.cols as i64)
                    }
                    "y" => top <= point_y && point_y < bottom,
                    "z" => placement.z_index == z,
                    "r" => range_ids.contains(&placement.id),
                    _ => false,
                }
            };
            if selected {
                affected.push(placement.id);
                removed.push(kitty.serial);
            }
            !selected
        });
        for serial in removed {
            result
                .events
                .push(TerminalGraphicsEvent::DeletePlacement { serial });
        }
        if matches!(selector.as_str(), "i" | "n") && !params.contains_key("p") {
            affected.extend(image);
        }
        if selector == "r" {
            affected.extend(range_ids);
        }
        affected.sort_unstable_by_key(|id| id.0);
        affected.dedup();
        if free_data {
            for id in affected {
                if self.kitty_placements.iter().all(|p| p.id != id) {
                    self.kitty_images.remove(&id);
                    result
                        .events
                        .push(TerminalGraphicsEvent::Delete { id: Some(id) });
                }
            }
        }
        Ok(())
    }

    fn delete_kitty_frame(
        &mut self,
        params: &HashMap<String, String>,
        result: &mut GraphicsAdvance,
    ) -> Result<(), GraphicsError> {
        let id = self.kitty_resource(params)?;
        let image = self
            .kitty_images
            .get_mut(&id)
            .ok_or(GraphicsError::ImageNotFound)?;
        ensure_kitty_animation_root(image);
        let index = parse_kitty_frame_index(params.get("r"))
            .unwrap_or(0)
            .min(image.frames.len() - 1);
        if image.frames.len() == 1 {
            if params.get("d").is_some_and(|d| d == "F") {
                result
                    .events
                    .push(TerminalGraphicsEvent::Delete { id: Some(id) });
            }
            return Ok(());
        }
        image.frames.remove(index);
        image.rgba = image.frames[0].rgba.clone();
        image.memory = image.frames[0].memory.clone();
        image.animation.current_frame = image
            .animation
            .current_frame
            .saturating_sub(usize::from(index < image.animation.current_frame))
            .min(image.frames.len() - 1);
        result
            .events
            .push(TerminalGraphicsEvent::ImageUpdated(image.clone()));
        Ok(())
    }
}
