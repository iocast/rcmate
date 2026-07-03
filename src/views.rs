pub mod about;
pub mod bisync_options;
pub mod edit;
pub mod main;

use crate::tui::View;

pub enum ViewAction {
    SwitchTo(View),
    OpenPopup(View),
    ClosePopup,
    Quit,
    None,
}
