use serde::{Serialize};
use crate::client_command::UserId;

#[derive(Debug, Serialize)]
pub struct ClientMessageResponse {
    pub id: u32, 
    #[serde(flatten)]
    pub response: ClientResponse,
}

#[derive(Debug, Serialize)]
#[serde(tag="response", rename_all="camelCase")]
pub enum ClientResponse {
    Success(ClientCommandResponse),
    Error(ClientCommandError),
}

#[derive(Debug, Serialize)]
#[serde(tag="command", rename_all="camelCase")]
pub enum ClientCommandResponse {
    SetAuthentication(AuthenticationContent),
    GetAuthentication,
    GetHoleCards,
    JoinTable,
    LeaveTable,
    Table,
}

#[derive(Debug, Serialize)]
#[serde(tag="error", rename_all="camelCase")]
pub enum ClientCommandError {
    Unknown,
    NotAuthenticated,
}

#[derive(Debug, Serialize)]
#[serde(tag="response", rename_all="camelCase")]
pub enum ClientAuthResponse {
    NotAuthenticated,
    Authenticated(AuthenticationContent),
    Error,
}

#[derive(Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct AuthenticationContent {
    pub user_id: UserId,
}