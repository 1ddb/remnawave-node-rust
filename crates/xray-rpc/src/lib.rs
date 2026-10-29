use std::collections::HashMap;
use std::os::unix::net::SocketAddr as StdUnixSocketAddr;
use std::sync::Arc;
use std::time::Duration;

use http::Uri;
use hyper_util::rt::TokioIo;
use prost::Message;
use thiserror::Error;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint};

pub mod xray {
    pub mod common {
        pub mod serial {
            tonic::include_proto!("xray.common.serial");
        }
        pub mod protocol {
            tonic::include_proto!("xray.common.protocol");
        }
    }

    pub mod app {
        pub mod stats {
            pub mod command {
                tonic::include_proto!("xray.app.stats.command");
            }
        }
        pub mod proxyman {
            pub mod command {
                tonic::include_proto!("xray.app.proxyman.command");
            }
        }
    }

    pub mod proxy {
        pub mod trojan {
            tonic::include_proto!("xray.proxy.trojan");
        }
        pub mod vless {
            tonic::include_proto!("xray.proxy.vless");
        }
        pub mod shadowsocks {
            tonic::include_proto!("xray.proxy.shadowsocks");
        }
        pub mod shadowsocks_2022 {
            tonic::include_proto!("xray.proxy.shadowsocks_2022");
        }
        pub mod hysteria {
            pub mod account {
                tonic::include_proto!("xray.proxy.hysteria.account");
            }
        }
    }
}

pub use xray::app::proxyman::command as proxyman_proto;
pub use xray::app::stats::command as stats_proto;
use xray::common::protocol::User;
use xray::common::serial::TypedMessage;
use proxyman_proto::handler_service_client::HandlerServiceClient;
use proxyman_proto::{AddUserOperation, AlterInboundRequest, RemoveOutboundRequest, RemoveUserOperation};
use stats_proto::stats_service_client::StatsServiceClient;
use stats_proto::{GetStatsRequest, GetUsersStatsRequest, QueryStatsRequest, SysStatsResponse};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
#[error("xray rpc error (code {code}): {message}")]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcError {
    pub fn is_not_found(&self) -> bool {
        self.code == 5 || self.message.contains("not found")
    }

    pub fn is_unimplemented(&self) -> bool {
        self.code == 12 || self.message.contains("unimplemented")
    }

    pub fn is_already_exists(&self) -> bool {
        self.message.contains("already exists")
    }
}

impl From<tonic::Status> for RpcError {
    fn from(status: tonic::Status) -> Self {
        Self {
            code: status.code() as i32,
            message: status.message().to_string(),
        }
    }
}

impl From<tonic::transport::Error> for RpcError {
    fn from(error: tonic::transport::Error) -> Self {
        Self {
            code: 14,
            message: error.to_string(),
        }
    }
}

/// gRPC client factory connected to Xray over an abstract unix socket.
#[derive(Clone)]
pub struct XrayRpc {
    channel: Channel,
}

impl XrayRpc {
    pub async fn connect_abstract(socket_path: &str) -> Result<Self, RpcError> {
        let addr = abstract_socket_addr(socket_path)?;
        let channel = Endpoint::try_from("http://localhost")
            .map_err(RpcError::from)?
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                let addr = addr.clone();
                async move {
                    let std_stream = std::os::unix::net::UnixStream::connect_addr(&addr)?;
                    std_stream.set_nonblocking(true)?;
                    let stream = UnixStream::from_std(std_stream)?;
                    Ok::<_, std::io::Error>(TokioIo::new(stream))
                }
            }))
            .await
            .map_err(RpcError::from)?;
        Ok(Self { channel })
    }

    fn stats_client(&self) -> StatsServiceClient<Channel> {
        StatsServiceClient::new(self.channel.clone())
            .max_decoding_message_size(100_000_000)
    }

    fn handler_client(&self) -> HandlerServiceClient<Channel> {
        HandlerServiceClient::new(self.channel.clone())
    }

    pub async fn get_sys_stats(&self) -> Result<SysStatsResponse, RpcError> {
        let mut client = self.stats_client();
        let response = client
            .get_sys_stats(stats_proto::SysStatsRequest {})
            .await
            .map_err(RpcError::from)?;
        Ok(response.into_inner())
    }

    pub async fn query_stats(
        &self,
        pattern: &str,
        reset: bool,
    ) -> Result<Vec<stats_proto::Stat>, RpcError> {
        let mut client = self.stats_client();
        let response = client
            .query_stats(QueryStatsRequest {
                pattern: pattern.to_string(),
                reset,
            })
            .await
            .map_err(RpcError::from)?;
        Ok(response.into_inner().stat)
    }

    pub async fn get_stats_online(&self, name: &str) -> Result<bool, RpcError> {
        let mut client = self.stats_client();
        let result = client
            .get_stats_online(GetStatsRequest {
                name: name.to_string(),
                reset: false,
            })
            .await;
        match result {
            Ok(_) => Ok(true),
            Err(status) => {
                let err = RpcError::from(status);
                if err.message.contains("online not found.") {
                    Ok(false)
                } else {
                    Err(err)
                }
            }
        }
    }

    pub async fn get_stats_online_ip_list(
        &self,
        name: &str,
        reset: bool,
    ) -> Result<HashMap<String, i64>, RpcError> {
        let mut client = self.stats_client();
        let response = client
            .get_stats_online_ip_list(GetStatsRequest {
                name: name.to_string(),
                reset,
            })
            .await
            .map_err(RpcError::from)?;
        Ok(response.into_inner().ips)
    }

    pub async fn get_all_online_users(&self) -> Result<Vec<String>, RpcError> {
        let mut client = self.stats_client();
        let response = client
            .get_all_online_users(stats_proto::GetAllOnlineUsersRequest {})
            .await
            .map_err(RpcError::from)?;
        Ok(response.into_inner().users)
    }

    /// Matches `@remnawave/xtls-sdk` behaviour: prefer `GetUsersStats`, fall
    /// back to `GetAllOnlineUsers` + `GetStatsOnlineIpList` on UNIMPLEMENTED.
    pub async fn get_users_stats(
        &self,
        include_traffic: bool,
        reset: bool,
    ) -> Result<Vec<stats_proto::UserStat>, RpcError> {
        let mut client = self.stats_client();
        match client
            .get_users_stats(GetUsersStatsRequest {
                include_traffic,
                reset,
            })
            .await
        {
            Ok(response) => Ok(response.into_inner().users),
            Err(status) => {
                let err = RpcError::from(status);
                if !err.is_unimplemented() {
                    return Err(err);
                }
                self.get_users_stats_legacy(reset).await
            }
        }
    }

    async fn get_users_stats_legacy(
        &self,
        reset: bool,
    ) -> Result<Vec<stats_proto::UserStat>, RpcError> {
        let online = self.get_all_online_users().await?;
        let mut users = Vec::new();
        for metric_name in online {
            let email = metric_name
                .strip_prefix("user>>>")
                .and_then(|s| s.strip_suffix(">>>online"))
                .unwrap_or(&metric_name)
                .to_string();
            let ips = self
                .get_stats_online_ip_list(&metric_name, reset)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(ip, last_seen)| stats_proto::OnlineIpEntry { ip, last_seen })
                .collect::<Vec<_>>();
            if ips.is_empty() {
                continue;
            }
            users.push(stats_proto::UserStat {
                email,
                ips,
                traffic: None,
            });
        }
        Ok(users)
    }

    pub async fn remove_user(&self, tag: &str, username: &str) -> Result<bool, RpcError> {
        let operation = RemoveUserOperation {
            email: username.to_string(),
        };
        self.alter_inbound(
            tag,
            typed_message("xray.app.proxyman.command.RemoveUserOperation", &operation),
        )
        .await
        .map(|_| true)
        .or_else(|err| if err.is_not_found() { Ok(true) } else { Err(err) })
    }

    async fn add_user(&self, tag: &str, user: User) -> Result<bool, RpcError> {
        let operation = AddUserOperation { user: Some(user) };
        self.alter_inbound(
            tag,
            typed_message("xray.app.proxyman.command.AddUserOperation", &operation),
        )
        .await
        .map(|_| true)
        .or_else(|err| {
            if err.is_already_exists() {
                Ok(true)
            } else {
                Err(err)
            }
        })
    }

    async fn alter_inbound(&self, tag: &str, operation: TypedMessage) -> Result<(), RpcError> {
        let mut client = self.handler_client();
        client
            .alter_inbound(AlterInboundRequest {
                tag: tag.to_string(),
                operation: Some(operation),
            })
            .await
            .map_err(RpcError::from)?;
        Ok(())
    }

    pub async fn add_trojan_user(
        &self,
        tag: &str,
        username: &str,
        password: &str,
        level: u32,
    ) -> Result<bool, RpcError> {
        let account = xray::proxy::trojan::Account {
            password: password.to_string(),
        };
        self.add_user(
            tag,
            build_user(
                username,
                level,
                "xray.proxy.trojan.Account",
                &account,
            ),
        )
        .await
    }

    pub async fn add_vless_user(
        &self,
        tag: &str,
        username: &str,
        uuid: &str,
        flow: &str,
        level: u32,
    ) -> Result<bool, RpcError> {
        let account = xray::proxy::vless::Account {
            id: uuid.to_string(),
            flow: flow.to_string(),
            encryption: "none".to_string(),
        };
        self.add_user(
            tag,
            build_user(username, level, "xray.proxy.vless.Account", &account),
        )
        .await
    }

    pub async fn add_shadowsocks_user(
        &self,
        tag: &str,
        username: &str,
        password: &str,
        cipher_type: i32,
        iv_check: bool,
        level: u32,
    ) -> Result<bool, RpcError> {
        let account = xray::proxy::shadowsocks::Account {
            password: password.to_string(),
            cipher_type,
            iv_check,
        };
        self.add_user(
            tag,
            build_user(
                username,
                level,
                "xray.proxy.shadowsocks.Account",
                &account,
            ),
        )
        .await
    }

    pub async fn add_shadowsocks2022_user(
        &self,
        tag: &str,
        username: &str,
        key: &str,
        level: u32,
    ) -> Result<bool, RpcError> {
        let account = xray::proxy::shadowsocks_2022::Account {
            key: key.to_string(),
        };
        self.add_user(
            tag,
            build_user(
                username,
                level,
                "xray.proxy.shadowsocks_2022.Account",
                &account,
            ),
        )
        .await
    }

    pub async fn add_hysteria_user(
        &self,
        tag: &str,
        username: &str,
        auth: &str,
        level: u32,
    ) -> Result<bool, RpcError> {
        let account = xray::proxy::hysteria::account::Account {
            auth: auth.to_string(),
        };
        self.add_user(
            tag,
            build_user(
                username,
                level,
                "xray.proxy.hysteria.account.Account",
                &account,
            ),
        )
        .await
    }

    pub async fn remove_outbound(&self, tag: &str) -> Result<(), RpcError> {
        let mut client = self.handler_client();
        client
            .remove_outbound(RemoveOutboundRequest {
                tag: tag.to_string(),
            })
            .await
            .map_err(RpcError::from)?;
        Ok(())
    }
}

fn build_user<M: Message>(email: &str, level: u32, type_name: &str, account: &M) -> User {
    User {
        level,
        email: email.to_string(),
        account: Some(typed_message(type_name, account)),
    }
}

fn typed_message<M: Message>(type_name: &str, message: &M) -> TypedMessage {
    TypedMessage {
        r#type: type_name.to_string(),
        value: message.encode_to_vec(),
    }
}

fn abstract_socket_addr(path: &str) -> Result<StdUnixSocketAddr, RpcError> {
    use std::os::linux::net::SocketAddrExt;
    StdUnixSocketAddr::from_abstract_name(path.as_bytes()).map_err(|e| RpcError {
        code: 14,
        message: e.to_string(),
    })
}

/// Convenience wrapper used by the app state.
pub type SharedXrayRpc = Arc<XrayRpc>;
