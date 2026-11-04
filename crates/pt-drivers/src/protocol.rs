use pt_json::Value;

use crate::emit::Out;
use crate::error::DriverError;
use crate::event::Decision;

/// One harness protocol as a pure state machine: messages and calls in, entries and stdin
/// lines out (through [`Out`]). No I/O, so the fuzz targets drive it directly.
pub(crate) trait Protocol: Send {
    fn start(&mut self, out: &mut Out);
    fn on_message(&mut self, msg: Value, out: &mut Out);
    /// A prompt, or with `steer` the native id of the turn it is meant for.
    fn send_user(
        &mut self,
        request_id: &str,
        text: &str,
        steer: Option<&str>,
        out: &mut Out,
    ) -> Result<(), DriverError>;
    fn respond_approval(
        &mut self,
        id: &str,
        decision: Decision,
        out: &mut Out,
    ) -> Result<(), DriverError>;
    fn interrupt(&mut self, out: &mut Out) -> Result<(), DriverError>;
    /// Compacts an idle session with the harness's own command. It ends in `Entry::Compaction`,
    /// or in a `compaction_failed` notice when the harness kept its context.
    fn compact(&mut self, _out: &mut Out) -> Result<(), DriverError> {
        Err(DriverError::Unsupported("compaction"))
    }
}
