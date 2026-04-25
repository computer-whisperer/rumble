//! Display model types shared between the live adapter and the
//! wireframe renderer. These are what the shell paints.
//!
//! Concrete state flows: `rumble_protocol::State` → `crate::adapters`
//! → these types → `crate::shell`.

#[derive(Clone, Debug)]
pub enum ChatEntry {
    Msg(ChatMsg),
    Sys(SysMsg),
}

#[derive(Clone, Debug)]
pub struct ChatMsg {
    pub who: String,
    pub t: String,
    pub body: Option<String>,
    pub media: Option<Media>,
}

#[derive(Clone, Debug)]
pub struct SysMsg {
    pub tone: SysTone,
    pub t: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysTone {
    Info,
    Join,
    Disc,
}

#[derive(Clone, Debug)]
pub enum Media {
    Image { name: String, size: String },
    File { ext: String, name: String, size: String },
}
