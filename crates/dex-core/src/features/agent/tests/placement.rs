//! Where a spawned agent's pane goes (issue #31). Six spawns from one lead
//! used to slice the lead's pane six times; now they take a strip beside it,
//! and a workspace arranged as a preset that grows well stays arranged.

use dex_protocol::agent::SpawnArgs;
use dex_protocol::workspace::{Layout, SplitDir, SplitDirection, WorkspaceArgs};

use super::pane;
use crate::app::AppState;
use crate::features::agent::placement::spawn_anchor;
use crate::features::agent::{event, spawn};
use crate::features::workspace;

fn brief(task: &str, pane: &str, direction: Option<&str>) -> SpawnArgs {
    SpawnArgs {
        task: task.into(),
        repo: None,
        worktree: None,
        label: None,
        direction: direction.map(str::to_owned),
        pane: Some(pane.into()),
        workspace: None,
    }
}

async fn lead(state: &AppState, pane: &str) {
    event(
        state,
        dex_protocol::agent::AgentEventArgs {
            kind: "session-start".into(),
            pane: pane.into(),
            agent: None,
            stamp: 1,
            input: serde_json::json!({ "session_id": "lead", "source": "startup" }),
        },
    )
    .await
    .unwrap();
}

async fn layout_of(state: &AppState) -> Layout {
    workspace::list(state).await.unwrap().workspaces[0]
        .layout
        .clone()
        .expect("a workspace has a layout")
}

fn leaves(layout: &Layout) -> Vec<String> {
    match layout {
        Layout::Leaf { pane_id } => vec![pane_id.clone()],
        Layout::Split { a, b, .. } => {
            let mut all = leaves(a);
            all.extend(leaves(b));
            all
        }
    }
}

#[test]
fn the_first_child_splits_the_caller_the_way_it_asked() {
    assert_eq!(
        spawn_anchor("lead", &[], None),
        ("lead".to_owned(), SplitDirection::Right)
    );
    assert_eq!(
        spawn_anchor("lead", &[], Some("down")),
        ("lead".to_owned(), SplitDirection::Down)
    );
}

#[test]
fn a_later_child_splits_the_newest_sibling_across_the_first_direction() {
    // Right of the lead, then stacked down beside it: a strip, not a slicing.
    let siblings = ["a".to_owned(), "b".to_owned()];
    assert_eq!(
        spawn_anchor("lead", &siblings, None),
        ("b".to_owned(), SplitDirection::Down)
    );
    // Asked for "down": below the lead, then a row along the bottom.
    assert_eq!(
        spawn_anchor("lead", &siblings, Some("down")),
        ("b".to_owned(), SplitDirection::Right)
    );
}

#[tokio::test]
async fn six_spawns_leave_the_lead_its_half_and_stack_the_agents_beside_it() {
    let (_dir, state, first) = pane().await;
    lead(&state, &first).await;
    let mut children = Vec::new();
    for n in 0..6 {
        let child = spawn(&state, brief(&format!("task {n}"), &first, None))
            .await
            .unwrap();
        children.push(child.pane);
    }
    let layout = layout_of(&state).await;
    // The lead is the left half of the root split, untouched by the five spawns after the first.
    let Layout::Split {
        dir,
        ratio,
        a: lead_side,
        b: agents,
    } = &layout
    else {
        panic!("{layout:?}");
    };
    assert_eq!(*dir, SplitDir::Horizontal);
    assert!((ratio - 0.5).abs() < 1e-9, "the lead keeps half: {ratio}");
    assert_eq!(
        **lead_side,
        Layout::Leaf {
            pane_id: first.clone()
        }
    );
    // All six agents are on the other side, in the order they were hired.
    assert_eq!(leaves(agents), children);
}

#[tokio::test]
async fn hires_by_the_owner_stack_the_same_way() {
    // No agent in the caller's pane: the owner, from HR or the CLI.
    let (_dir, state, first) = pane().await;
    let a = spawn(&state, brief("one", &first, None)).await.unwrap();
    let b = spawn(&state, brief("two", &first, None)).await.unwrap();
    let layout = layout_of(&state).await;
    let Layout::Split {
        a: owner_side,
        b: agents,
        ..
    } = &layout
    else {
        panic!("{layout:?}");
    };
    assert_eq!(
        **owner_side,
        Layout::Leaf {
            pane_id: first.clone()
        }
    );
    assert_eq!(leaves(agents), vec![a.pane, b.pane]);
}

#[tokio::test]
async fn a_workspace_arranged_as_a_grid_stays_a_grid_as_agents_arrive() {
    let (_dir, state, first) = pane().await;
    lead(&state, &first).await;
    for n in 0..3 {
        spawn(&state, brief(&format!("task {n}"), &first, None))
            .await
            .unwrap();
    }
    // The owner cycles to the tiled preset.
    let ws = workspace::list(&state).await.unwrap().workspaces[0]
        .id
        .clone();
    let mut tiled = None;
    for _ in 0..5 {
        let list = workspace::cycle_layout(
            &state,
            WorkspaceArgs {
                workspace: ws.clone(),
            },
        )
        .await
        .unwrap();
        let layout = list.workspaces[0].layout.clone().unwrap();
        let ids = leaves(&layout);
        if Some(&layout)
            == crate::features::workspace::preset_layout(
                crate::features::workspace::Preset::Tiled,
                &ids,
            )
            .as_ref()
        {
            tiled = Some(layout);
            break;
        }
    }
    assert!(tiled.is_some(), "the cycle reaches the grid");

    let fifth = spawn(&state, brief("task 4", &first, None)).await.unwrap();

    let layout = layout_of(&state).await;
    let ids = leaves(&layout);
    assert!(ids.contains(&fifth.pane));
    assert_eq!(
        Some(&layout),
        crate::features::workspace::preset_layout(crate::features::workspace::Preset::Tiled, &ids)
            .as_ref(),
        "still the grid, one pane bigger"
    );
}

#[tokio::test]
async fn a_layout_arranged_by_hand_is_not_rearranged_by_a_spawn() {
    let (_dir, state, first) = pane().await;
    lead(&state, &first).await;
    let a = spawn(&state, brief("one", &first, None)).await.unwrap();
    // The owner drags the divider: no preset has this ratio.
    let hand = Layout::Split {
        dir: SplitDir::Horizontal,
        ratio: 0.33,
        a: Box::new(Layout::Leaf {
            pane_id: first.clone(),
        }),
        b: Box::new(Layout::Leaf {
            pane_id: a.pane.clone(),
        }),
    };
    let ws = workspace::list(&state).await.unwrap().workspaces[0]
        .id
        .clone();
    workspace::set_layout(
        &state,
        dex_protocol::workspace::SetLayoutArgs {
            workspace: ws,
            layout: hand,
        },
    )
    .await
    .unwrap();

    let b = spawn(&state, brief("two", &first, None)).await.unwrap();

    let layout = layout_of(&state).await;
    let Layout::Split {
        ratio,
        a: lead_side,
        b: agents,
        ..
    } = &layout
    else {
        panic!("{layout:?}");
    };
    assert!(
        (ratio - 0.33).abs() < 1e-9,
        "the owner's divider stays where they put it: {ratio}"
    );
    assert_eq!(**lead_side, Layout::Leaf { pane_id: first });
    assert_eq!(leaves(agents), vec![a.pane, b.pane]);
}

/// The two panes a leaf's parent split holds, if `pane` is one of them.
fn beside(layout: &Layout, pane: &str) -> Option<(Layout, Layout)> {
    match layout {
        Layout::Leaf { .. } => None,
        Layout::Split { a, b, .. } => {
            let here = |node: &Layout| matches!(node, Layout::Leaf { pane_id } if pane_id == pane);
            if here(a) || here(b) {
                return Some(((**a).clone(), (**b).clone()));
            }
            beside(a, pane).or_else(|| beside(b, pane))
        }
    }
}

#[tokio::test]
async fn an_owner_hire_never_splits_an_agent_the_owner_started_by_hand() {
    // A lead the owner started by typing `claude` in the first pane; the owner
    // hires from HR with the second pane focused. The hire goes beside the
    // second pane - not beside the lead, who is nobody's hire.
    let (_dir, state, first) = pane().await;
    lead(&state, &first).await;
    let list = workspace::split_pane(
        &state,
        dex_protocol::workspace::SplitPaneArgs {
            pane: first.clone(),
            direction: SplitDirection::Right,
            cwd: None,
            label: None,
            kind: None,
        },
    )
    .await
    .unwrap();
    let second = list.workspaces[0].active_pane.clone().unwrap();

    let hire = spawn(&state, brief("one", &second, None)).await.unwrap();

    let layout = layout_of(&state).await;
    let (a, b) = beside(&layout, &hire.pane).expect("the hire is in a split");
    let partner = if a
        == (Layout::Leaf {
            pane_id: hire.pane.clone(),
        }) {
        b
    } else {
        a
    };
    assert_eq!(partner, Layout::Leaf { pane_id: second });
}
