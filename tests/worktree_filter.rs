use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use ump_dash::app::{AppState, active_worktree_id, handle_key, update};
use ump_dash::domain::action::Action;
use ump_dash::domain::agent_protocol::{AgentRequest, AgentRequestId};
use ump_dash::domain::worktree::{Worktree, WorktreeId, WorktreeMetroStatus};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn worktree(id: &str, branch: &str, title: Option<&str>) -> Worktree {
    Worktree {
        id: WorktreeId(id.into()),
        path: format!("/tmp/{id}").into(),
        branch: branch.into(),
        head_sha: "0000000".into(),
        metro_status: WorktreeMetroStatus::Stopped,
        jira_title: title.map(str::to_string),
        stale: false,
        stale_pods: false,
        jira_key: None,
    }
}

fn rendered_view(state: &mut AppState) -> String {
    let backend = TestBackend::new(100, 12);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| ump_dash::ui::view(frame, state))
        .expect("worktree view should render");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn slash_enters_worktree_filter_input() {
    let mut state = AppState::default();

    let search = handle_key(&state, key(KeyCode::Char('/'))).expect("/ should start filtering");
    assert_eq!(search, Action::Search);
    let effects = update(&mut state, search);

    assert!(effects.is_empty());
    assert!(state.worktree_browser.filter_input_active);
    assert_eq!(state.worktree_browser.filter_query, "");
    assert_eq!(
        handle_key(&state, key(KeyCode::Char('q'))),
        Some(Action::WorktreeFilterInput('q'))
    );
}

#[test]
fn typed_query_filters_rendered_worktrees_case_insensitively() {
    let mut state = AppState::default();
    let _ = update(
        &mut state,
        Action::WorktreesLoaded(vec![
            worktree("UMP-100-login", "feature/auth", Some("Customer Login")),
            worktree("UMP-200-payments", "feature/payments", Some("Checkout")),
        ]),
    );
    let _ = update(&mut state, Action::Search);
    for c in "LoGiN".chars() {
        let _ = update(&mut state, Action::WorktreeFilterInput(c));
    }

    let rendered = rendered_view(&mut state);

    assert!(rendered.contains("UMP-100-login"));
    assert!(!rendered.contains("UMP-200-payments"));
}

#[test]
fn filter_can_be_edited_applied_and_cleared() {
    let mut state = AppState::default();
    let _ = update(&mut state, Action::Search);
    for c in "login".chars() {
        let _ = update(&mut state, Action::WorktreeFilterInput(c));
    }

    let backspace = handle_key(&state, key(KeyCode::Backspace)).expect("backspace should edit");
    assert_eq!(backspace, Action::WorktreeFilterBackspace);
    let _ = update(&mut state, backspace);
    assert_eq!(state.worktree_browser.filter_query, "logi");

    let apply = handle_key(&state, key(KeyCode::Enter)).expect("enter should apply");
    assert_eq!(apply, Action::WorktreeFilterApply);
    let _ = update(&mut state, apply);
    assert!(!state.worktree_browser.filter_input_active);
    assert_eq!(state.worktree_browser.filter_query, "logi");

    let _ = update(&mut state, Action::Search);
    let clear = handle_key(&state, key(KeyCode::Esc)).expect("escape should clear");
    assert_eq!(clear, Action::WorktreeFilterClear);
    let _ = update(&mut state, clear);
    assert!(!state.worktree_browser.filter_input_active);
    assert!(state.worktree_browser.filter_query.is_empty());
}

#[test]
fn commands_target_the_visible_filtered_worktree() {
    let mut state = AppState::default();
    let _ = update(
        &mut state,
        Action::WorktreesLoaded(vec![
            worktree("UMP-100-login", "feature/auth", Some("Customer Login")),
            worktree("UMP-200-payments", "feature/payments", Some("Checkout")),
        ]),
    );
    let _ = update(&mut state, Action::Search);
    for c in "payments".chars() {
        let _ = update(&mut state, Action::WorktreeFilterInput(c));
    }
    let _ = update(&mut state, Action::WorktreeFilterApply);

    assert_eq!(
        active_worktree_id(&state),
        Some(WorktreeId("UMP-200-payments".into()))
    );

    let _ = update(&mut state, Action::WorktreeRemove);
    let (pending_id, _, _) = state
        .modal_stack
        .pending_worktree_removal
        .expect("removal should target the visible worktree");
    assert_eq!(pending_id, WorktreeId("UMP-200-payments".into()));
}

#[test]
fn navigation_moves_between_visible_worktrees() {
    let mut state = AppState::default();
    let _ = update(
        &mut state,
        Action::WorktreesLoaded(vec![
            worktree("alpha", "feature/client-alpha", None),
            worktree("beta", "feature/server-beta", None),
            worktree("gamma", "feature/client-gamma", None),
        ]),
    );
    let _ = update(&mut state, Action::Search);
    for c in "client".chars() {
        let _ = update(&mut state, Action::WorktreeFilterInput(c));
    }

    let _ = update(&mut state, Action::WorktreeSelectNext);

    assert_eq!(active_worktree_id(&state), Some(WorktreeId("gamma".into())));
    assert_eq!(
        state.worktree_browser.selected_worktree_id,
        Some(WorktreeId("gamma".into()))
    );
    assert_eq!(
        state.metro_state.active_worktree_path.as_deref(),
        Some(std::path::Path::new("/tmp/gamma"))
    );
}

#[test]
fn narrowing_filter_reselects_the_first_visible_worktree() {
    let mut state = AppState::default();
    let _ = update(
        &mut state,
        Action::WorktreesLoaded(vec![
            worktree("alpha", "feature/login", None),
            worktree("beta", "feature/payments", None),
        ]),
    );
    let _ = update(&mut state, Action::WorktreeSelectNext);
    assert_eq!(active_worktree_id(&state), Some(WorktreeId("beta".into())));

    let _ = update(&mut state, Action::Search);
    for c in "login".chars() {
        let _ = update(&mut state, Action::WorktreeFilterInput(c));
    }

    assert_eq!(active_worktree_id(&state), Some(WorktreeId("alpha".into())));
    assert_eq!(
        state.worktree_browser.selected_worktree_id,
        Some(WorktreeId("alpha".into()))
    );
    assert_eq!(
        state.metro_state.active_worktree_path.as_deref(),
        Some(std::path::Path::new("/tmp/alpha"))
    );
}

#[test]
fn active_filter_query_is_visible_above_the_table() {
    let mut state = AppState::default();
    let _ = update(
        &mut state,
        Action::WorktreesLoaded(vec![worktree(
            "alpha",
            "feature/login",
            Some("Customer Login"),
        )]),
    );
    let _ = update(&mut state, Action::Search);
    for c in "CUSTOMER".chars() {
        let _ = update(&mut state, Action::WorktreeFilterInput(c));
    }

    let rendered = rendered_view(&mut state);

    assert!(rendered.contains("/CUSTOMER"));
}

fn assert_selection(state: &AppState, expected: Option<(usize, &str)>) {
    assert_eq!(
        state.worktree_browser.worktree_table_state.selected(),
        expected.map(|(index, _)| index)
    );
    let expected_id = expected.map(|(_, id)| WorktreeId(id.into()));
    assert_eq!(state.worktree_browser.selected_worktree_id, expected_id);
    assert_eq!(active_worktree_id(state), expected_id);
    assert_eq!(
        state.metro_state.active_worktree_path,
        expected.map(|(_, id)| format!("/tmp/{id}").into())
    );
}

#[test]
fn removal_selects_previous_visible_worktree_and_preserves_it_after_refresh() {
    for (filter, selected_index, expected_index, expected_id) in [
        ("", 2, 1, "beta"),
        ("", 3, 2, "gamma"),
        ("", 0, 0, "beta"),
        ("client", 2, 1, "gamma"),
        ("client", 0, 0, "gamma"),
    ] {
        for via_agent in [false, true] {
            let mut state = AppState::default();
            state.worktree_browser.filter_query = filter.into();
            let _ = update(
                &mut state,
                Action::WorktreesLoaded(vec![
                    worktree("alpha", "feature/client-alpha", None),
                    worktree("beta", "feature/server-beta", None),
                    worktree("gamma", "feature/client-gamma", None),
                    worktree("delta", "feature/client-delta", None),
                ]),
            );
            for _ in 0..selected_index {
                let _ = update(&mut state, Action::WorktreeSelectNext);
            }
            let removed_path = state.metro_state.active_worktree_path.clone().unwrap();

            if via_agent {
                let _ = update(
                    &mut state,
                    Action::Agent {
                        request_id: AgentRequestId(1),
                        cwd: removed_path.clone(),
                        request: AgentRequest::DeleteWorktree {
                            target_worktree: removed_path.to_string_lossy().into(),
                            confirm: true,
                        },
                    },
                );
            } else {
                let _ = update(&mut state, Action::WorktreeRemove);
                let _ = update(&mut state, Action::ModalConfirm);
            }
            assert!(
                !state
                    .worktree_browser
                    .worktrees
                    .iter()
                    .any(|wt| wt.path == removed_path)
            );
            assert_selection(&state, Some((expected_index, expected_id)));

            let _ = update(
                &mut state,
                Action::WorktreeRemoved(removed_path.to_string_lossy().into()),
            );
            let remaining = state.worktree_browser.worktrees.clone();
            let _ = update(&mut state, Action::WorktreesLoaded(remaining));
            assert_selection(&state, Some((expected_index, expected_id)));
        }
    }
}

#[test]
fn removing_only_visible_worktree_clears_selection() {
    for with_hidden_worktree in [false, true] {
        let mut state = AppState::default();
        state.worktree_browser.filter_query = "client".into();
        let mut worktrees = vec![worktree("alpha", "feature/client-alpha", None)];
        if with_hidden_worktree {
            worktrees.push(worktree("beta", "feature/server-beta", None));
        }
        let _ = update(&mut state, Action::WorktreesLoaded(worktrees));

        let _ = update(&mut state, Action::WorktreeRemove);
        let _ = update(&mut state, Action::ModalConfirm);

        assert_selection(&state, None);
    }
}

#[test]
fn removing_another_worktree_preserves_selection() {
    let mut state = AppState::default();
    let _ = update(
        &mut state,
        Action::WorktreesLoaded(vec![
            worktree("alpha", "feature/alpha", None),
            worktree("beta", "feature/beta", None),
            worktree("gamma", "feature/gamma", None),
        ]),
    );
    let _ = update(&mut state, Action::WorktreeRemove);
    let _ = update(&mut state, Action::WorktreeSelectNext);
    let _ = update(&mut state, Action::WorktreeSelectNext);

    let _ = update(&mut state, Action::ModalConfirm);

    assert_selection(&state, Some((1, "gamma")));
}
