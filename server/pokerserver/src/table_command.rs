use tokio::sync::mpsc;
use crate::client_command::{UserId, TableAction};
use pokercore::GameEvent;

pub struct TableCommand {
    pub user_id: UserId,
    pub cmd: TableCommands,
}

pub enum TableCommands {
    Join(JoinTableCommand),
    Leave,
    Action(ActionTableCommand),
    /// Test-only: run one `Table::process()` tick on demand. Sent by the
    /// `/test/tick` handler, which only exists when `POKER_TEST_HOOKS=1`.
    Tick,
}

pub struct JoinTableCommand {
    pub tx: mpsc::Sender<GameEvent>
}

pub struct ActionTableCommand {
    pub action: TableAction,
}