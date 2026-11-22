use serde::{Serialize, Deserialize};
use uuid::{Uuid};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub struct UserId(pub Uuid);

#[derive(Debug, Deserialize)]
pub struct TableId(pub Uuid);

#[derive(Debug, Deserialize)]
pub struct ClientMessage {
    pub id: u32, 
    #[serde(flatten)]
    pub command: ClientCommand,
}

#[derive(Debug, Deserialize)]
#[serde(tag="command", rename_all="camelCase")]
pub enum ClientCommand {
    SetAuthentication(SetAuthenticationCommand),
    GetAuthentication,
    GetHoleCards,
    JoinTable(JoinTableClientCommand),
    LeaveTable,
    Table(ClientTableCommand),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SetAuthenticationCommand {
    pub user_id: UserId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct JoinTableClientCommand {
    table_id: TableId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ClientTableCommand {
    pub table_id: TableId,
    pub action: TableAction,
}

#[derive(Debug, Deserialize)]
#[serde(tag="action", rename_all="camelCase")]
pub enum TableAction {
    Sit(SitAction),
    Stand,
    Bet(BetAction),
    Check,
    Fold,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SitAction {
    pub bank_amount: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct BetAction {
    pub bet_amount: u32,
}

