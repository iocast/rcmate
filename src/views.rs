pub mod about;
pub mod bisync_options;
pub mod edit;
pub mod main;
pub mod progress;

use crate::tui::View;

pub enum ViewAction {
    SwitchTo(View),
    OpenPopup(View),
    ClosePopup,
    Quit,
    None,
}
