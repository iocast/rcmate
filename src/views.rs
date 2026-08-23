pub mod about;
pub mod bisync_options;
pub mod main;
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
