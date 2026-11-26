//! Keys held down: the keyboard sends a held key again and again, and GPUI
//! dispatches each repeat to the key's binding. The Tauri app runs a command
//! once a press unless it asks to repeat (`repeat` in
//! `src/app/use-app-commands.ts`): a held ⌘K toggled the palette at every
//! repeat, a held ⌘W closed the tab and then the window. The same rule holds
//! here for every key, an island's forwarded ones included
//! (`lattice_webview::keys`): a repeat of the app's own commands is still
//! taken, so it never types into the editor, but runs nothing
//! (`App::set_held_key_repeats`, `gpui-fast-held-keys.diff`).

use gpui::Action;

/// The app's commands: the shell's, the palette's and menus', the panels'
/// and documents' views, and the AI's.
const APP_NAMESPACES: [&str; 4] = [
    "lattice",
    "lattice_document",
    "lattice_editor_panel",
    "lattice_proofread",
];

/// Of those, the ones worth running again while their key stays down, as
/// the Tauri table's `repeat: true`: a step through history or the build's
/// problems, a zoom. The LaTeX source's own keys are the editor's, which
/// CodeMirror's keymap repeats there: typing rules, formatting, case and
/// selection.
const REPEATS: &[&str] = &[
    "GoBack",
    "GoForward",
    "NavigateBack",
    "NavigateForward",
    "NextProblem",
    "PreviousProblem",
    "LargerInterface",
    "SmallerInterface",
    "TypeDollar",
    "LatexNewline",
    "ToggleBold",
    "ToggleEmphasis",
    "ToggleInlineMath",
    "ToggleLineComments",
    "WrapCommentEnvironment",
    "WrapIfFalse",
    "WrapEquation",
    "WrapItemize",
    "SortLines",
    "UpperCase",
    "LowerCase",
    "TitleCase",
    "SelectEnvironment",
    "MatchingEnvironment",
];

/// Whether `action` runs again for a held key's repeat. Every action outside
/// the app's commands does: the editor's caret moves, deletes, undo and
/// typing, Vim's, a list's ↑ and ↓.
pub fn repeats(action: &dyn Action) -> bool {
    let Some((namespace, name)) = action.name().rsplit_once("::") else {
        return true;
    };
    !APP_NAMESPACES.contains(&namespace) || REPEATS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_panel::{latex_format, latex_input};
    use crate::shell::commands::*;

    #[test]
    fn commands_that_open_close_or_start_something_run_once_a_press() {
        for action in [
            &CommandPalette as &dyn Action,
            &CloseTab,
            &ReopenClosedTab,
            &Save,
            &QuickOpen,
            &FindInProject,
            &ToggleProjectPanel,
            &ToggleAgentPanel,
            &ShowShortcuts,
            &OpenSettings,
            &CloseWindow,
            &ResetInterfaceSize,
            &crate::terminal::ToggleTerminalPanel,
            &crate::terminal::CloseTerminalTab,
            &crate::shell::GoToLine,
            &crate::editor_panel::proofread::ProofreadSelection,
        ] {
            assert!(!repeats(action), "{} repeats", action.name());
        }
    }

    #[test]
    fn history_problems_and_zoom_repeat_as_in_the_tauri_app() {
        for action in [
            &GoBack as &dyn Action,
            &GoForward,
            &NavigateBack,
            &NavigateForward,
            &NextProblem,
            &PreviousProblem,
            &LargerInterface,
            &SmallerInterface,
        ] {
            assert!(repeats(action), "{} runs once", action.name());
        }
    }

    #[test]
    fn the_editors_own_keys_repeat() {
        for action in [
            &editor::actions::MoveLeft as &dyn Action,
            &editor::actions::Backspace,
            &editor::actions::Undo,
            &editor::actions::Redo,
            &latex_input::TypeDollar,
            &latex_input::LatexNewline,
            &latex_format::ToggleBold,
            &latex_format::MatchingEnvironment,
            &crate::editor_panel::find_bar::FindNext,
            &crate::editor_panel::NextReviewItem,
        ] {
            assert!(repeats(action), "{} runs once", action.name());
        }
    }

    #[test]
    fn every_repeating_name_is_one_of_the_apps_commands() {
        let names: Vec<_> = gpui::generate_list_of_all_registered_actions()
            .map(|action| action.name)
            .collect();
        for name in REPEATS {
            assert!(
                names.iter().any(|full| full.rsplit_once("::").is_some_and(
                    |(namespace, short)| short == *name && APP_NAMESPACES.contains(&namespace)
                )),
                "{name} is none of the app's commands"
            );
        }
    }
}
