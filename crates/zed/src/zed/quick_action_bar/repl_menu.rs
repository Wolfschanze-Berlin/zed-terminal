use ui::{AnyElement, prelude::*};

use super::QuickActionBar;

impl QuickActionBar {
    pub fn render_repl_menu(&self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        None
    }
}
