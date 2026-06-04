use ump_dash::domain::{
    ports::metro_port::MetroHandle, worktree::WorktreeId, worktree_slice::WorktreeSlice,
};

#[derive(Debug)]
struct FakeMetroHandle {
    pid: u32,
    worktree_id: String,
    port: u16,
}

impl MetroHandle for FakeMetroHandle {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn worktree_id(&self) -> &str {
        &self.worktree_id
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
        Ok(())
    }

    fn kill(self: Box<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}

fn fake_metro_handle(pid: u32, worktree_id: &str, port: u16) -> Box<dyn MetroHandle> {
    Box::new(FakeMetroHandle {
        pid,
        worktree_id: worktree_id.to_string(),
        port,
    })
}

#[test]
fn default_worktree_slice_owns_a_stopped_worktree_metro() {
    let slice = WorktreeSlice::default();

    assert!(!slice.metro.is_running());
    assert_eq!(slice.metro.running_port(), None);
}

#[test]
fn metro_process_and_port_state_are_scoped_to_each_worktree_slice() {
    let mut slice_a = WorktreeSlice {
        id: WorktreeId("wt-a".into()),
        ..Default::default()
    };
    let mut slice_b = WorktreeSlice {
        id: WorktreeId("wt-b".into()),
        ..Default::default()
    };

    slice_a
        .metro
        .register(fake_metro_handle(9001, "wt-a", 8081));
    slice_b
        .metro
        .register(fake_metro_handle(9002, "wt-b", 8082));

    assert!(slice_a.metro.is_running());
    assert!(slice_b.metro.is_running());
    assert_eq!(slice_a.metro.running_port(), Some(8081));
    assert_eq!(slice_b.metro.running_port(), Some(8082));

    let stopped_handle = slice_a
        .metro
        .take_handle()
        .expect("slice A should own its metro process handle");

    assert_eq!(stopped_handle.pid(), 9001);
    assert!(!slice_a.metro.is_running());
    assert_eq!(slice_a.metro.running_port(), None);
    assert!(slice_b.metro.is_running());
    assert_eq!(slice_b.metro.running_port(), Some(8082));
}
