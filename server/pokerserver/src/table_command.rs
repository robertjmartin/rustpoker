use tokio::sync::mpsc;
use crate::client_command::{UserId, TableAction};

pub struct TableCommand {
    pub user_id: UserId,
    pub cmd: TableCommands,
}

pub enum TableCommands {
    Join(JoinTableCommand),
    Leave,
    /// The WebSocket task ended: stand this user up so an abandoned seat
    /// cannot hold the table up.
    Disconnect(DisconnectCommand),
    /// Resend this user's hole cards as a private message.
    HoleCards,
    Action(ActionTableCommand),
}

pub struct JoinTableCommand {
    /// One JSON-encoded `GameEvent` per message.
    pub tx: mpsc::Sender<String>
}

pub struct DisconnectCommand {
    /// The connection that went away. If a newer socket has registered for
    /// the same user since, the worker keeps that connection and its seat.
    pub tx: mpsc::Sender<String>,
}

pub struct ActionTableCommand {
    pub action: TableAction,
}