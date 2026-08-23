pub mod about;
pub mod main;
pub mod options;
pub mod progress;
pub mod sync_pair_form;

use crate::tui::View;

pub enum ViewAction {
    SwitchTo(View),
    OpenPopup(View),
    ClosePopup,
    Quit,
    None,
}
