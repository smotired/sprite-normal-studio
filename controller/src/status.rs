use crate::Controller;

/// What the status bar shows: the result of the latest tool action, or feedback from the app
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    /// Nothing to report
    #[default]
    Ok,

    /// A message about something that worked, e.g. a file was saved
    Feedback(String),

    /// Why the latest tool action failed
    Error(String),
}

impl Controller {
    /// What the status bar should show
    pub fn status(&self) -> &Status { &self.status }

    /// Show a message about something that worked. The next tool action replaces it.
    pub fn set_feedback(&mut self, text: impl Into<String>) {
        self.status = Status::Feedback(text.into());
    }
}
