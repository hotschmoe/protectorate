use pt_frame::msg::ErrorCode;

#[derive(Debug)]
pub struct JobError {
    pub code: ErrorCode,
    pub message: String,
}

impl JobError {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> JobError {
        JobError {
            code,
            message: message.into(),
        }
    }
}
