#[path = "../src/demo/shell.rs"]
mod shell;
#[path = "../src/demo/storage.rs"]
mod storage;

#[test]
fn file_actions_change_only_the_addressed_shell_and_leave_pending_input_intact() {
    let files = storage::DemoFiles::seeded();
    let folder = "/srv/oxideterm/build notes";
    files.borrow_mut().put(folder, None);
    let mut a = shell::Shell::new(files.clone(), storage::REMOTE);
    let mut b = shell::Shell::new(files, storage::REMOTE);
    b.input(b"echo keep");
    assert!(b.change_directory(folder).is_err());
    a.change_directory(folder).unwrap();
    assert_eq!(
        a.input(b"pwd\r").as_slice(),
        b"pwd\r\n/srv/oxideterm/build notes\r\ndemo@oxide:/srv/oxideterm/build notes$ "
    );
    assert_eq!(
        b.input(b"\r").as_slice(),
        b"\r\nkeep\r\ndemo@oxide:/srv/oxideterm$ "
    );
}
