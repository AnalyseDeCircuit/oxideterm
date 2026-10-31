use oxideterm_image_decoder::reserve_image_memory;
use oxideterm_terminal_graphics::{
    GraphicsCursor, GraphicsIngress, GraphicsOptions, TerminalGraphicsEvent,
};

#[test]
fn sessions_share_the_allowance_and_snapshots_keep_pixels_charged() {
    // A separate test process can exhaust the real allowance without racing unit decoders.
    let _occupied = reserve_image_memory(512 * 1024 * 1024 - 8).unwrap();
    let cursor = GraphicsCursor {
        line: 0,
        row: 0,
        col: 0,
        cols: 80,
        rows: 24,
        cell_width: 10,
        cell_height: 20,
    };
    let mut first = GraphicsIngress::new(GraphicsOptions::default());
    let first_output = first.advance(b"\x1b_Ga=t,i=1,f=32,s=1,v=1;ESIz/w==\x1b\\", cursor);
    let pixels = first_output
        .events
        .iter()
        .find_map(|event| match event {
            TerminalGraphicsEvent::ImageReady(image) => Some(image.rgba.as_ref()),
            _ => None,
        })
        .unwrap();
    assert_eq!(pixels, &[17, 34, 51, 255]);
    let mut second = GraphicsIngress::new(GraphicsOptions::default());
    let upload = b"\x1b_Ga=t,i=2,f=32,s=1,v=1;AP8A/w==\x1b\\";
    let assert_denied = |events: &[TerminalGraphicsEvent]| {
        assert!(events.contains(&TerminalGraphicsEvent::Respond(
            b"\x1b_Gi=2;ENOSPC:Graphics resource limit exceeded\x1b\\".to_vec()
        )));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::ImageReady(_)))
        );
    };
    assert_denied(&second.advance(upload, cursor).events);
    drop(first);
    assert_denied(&second.advance(upload, cursor).events);
    drop(first_output);
    let accepted = second.advance(upload, cursor);
    let image = accepted
        .events
        .iter()
        .find_map(|event| match event {
            TerminalGraphicsEvent::ImageReady(image) => Some(image),
            _ => None,
        })
        .unwrap();
    assert_eq!(image.rgba.as_ref(), &[0, 255, 0, 255]);
}
