use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};
use serenity::model::channel::{Channel, Message};
use serenity::model::guild::Member;
use serenity::model::id::{ChannelId, GuildId, MessageId};
use serenity::model::prelude::User;
use serenity::prelude::*;
use serenity::Result;
use tokio::fs::{symlink, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{MappedMutexGuard, MutexGuard};

pub struct Logger {
    channel_logs: Mutex<HashMap<ChannelId, File>>,
}

impl Logger {
    pub fn new() -> Self {
        Logger {
            channel_logs: Mutex::default(),
        }
    }

    async fn get_log_file(
        &self,
        ctx: &Context,
        channel: ChannelId,
    ) -> Result<MappedMutexGuard<'_, File>> {
        let mut logs = self.channel_logs.lock().await;
        if let Entry::Vacant(e) = logs.entry(channel) {
            let mut log = PathBuf::new();
            log.push("logs");
            tokio::fs::create_dir_all(&log).await?;
            match channel.to_channel(ctx).await? {
                Channel::Guild(channel) => {
                    log.push(format!("#{}.log", channel.id));
                    let mut log_link = PathBuf::from("servers");
                    log_link.push(escape(&channel.guild_id.to_partial_guild(ctx).await?.name));
                    tokio::fs::create_dir_all(&log_link).await?;
                    log_link.push(format!("{}.log", escape(&channel.name)));
                    let _ = symlink(Path::new("../../").join(&log), log_link).await;
                }
                Channel::Private(channel) => {
                    log.push(format!("@{}.log", channel.recipient.id));
                    let mut log_link = PathBuf::from("users");
                    tokio::fs::create_dir_all(&log_link).await?;
                    log_link.push(format!("{}.log", escape(&channel.recipient.tag())));
                    let _ = symlink(Path::new("../").join(&log), log_link).await;
                }
                _ => return Err(serenity::Error::Other("unrecognized channel type")),
            }
            e.insert(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log)
                    .await?,
            );
        }
        Ok(MutexGuard::map(logs, |map| map.get_mut(&channel).unwrap()))
    }
}

#[serenity::async_trait]
impl EventHandler for Logger {
    async fn message(&self, ctx: Context, message: Message) {
        let _: Result<()> = async {
            let flagged = is_nitro_scam(&message.content).await;

            let mut data = String::new();
            writeln!(
                &mut data,
                "MESSAGE {} &{} <{}> @{} ({}): {}",
                match flagged {
                    true => "FLAGGED",
                    false => "",
                },
                message.id,
                message.timestamp.to_rfc3339(),
                message.author.id,
                escape(&message.author.tag()),
                escape(&message.content)
            )?;

            for attachment in &message.attachments {
                writeln!(
                    &mut data,
                    "ATTACHED {} ({}): {}",
                    attachment.content_type.as_deref().unwrap_or("unknown"),
                    escape(&attachment.filename),
                    attachment.url
                )?;
            }

            let mut log = self.get_log_file(&ctx, message.channel_id).await?;
            log.write_all(data.as_bytes()).await?;

            if flagged {
                message.delete(&ctx).await?;
            }

            Ok(())
        }
        .await;
    }

    async fn message_delete(
        &self,
        ctx: Context,
        channel: ChannelId,
        message: MessageId,
        _guild_id: Option<GuildId>,
    ) {
        let _: Result<()> = async {
            let mut data = String::new();
            writeln!(
                &mut data,
                "DELETE   &{} <{}>",
                message,
                chrono::Utc::now().to_rfc3339(),
            )?;

            let mut log = self.get_log_file(&ctx, channel).await?;
            log.write_all(data.as_bytes()).await?;
            Ok(())
        }
        .await;
    }

    async fn message_update(
        &self,
        ctx: Context,
        _old_if_available: Option<Message>,
        _new: Option<Message>,
        event: serenity::model::event::MessageUpdateEvent,
    ) {
        if event.content.is_none() {
            return;
        }

        let _: Result<()> = async {
            let mut data = String::new();
            writeln!(
                &mut data,
                "EDIT     &{} <{}>: {}",
                event.id,
                event
                    .edited_timestamp
                    .unwrap_or_else(chrono::Utc::now)
                    .to_rfc3339(),
                escape(&event.content.unwrap())
            )?;

            let mut log = self.get_log_file(&ctx, event.channel_id).await?;
            log.write_all(data.as_bytes()).await?;
            Ok(())
        }
        .await;
    }

    async fn guild_member_addition(&self, ctx: Context, server: GuildId, new_member: Member) {
        let _: Result<()> = async {
            let channel = match server.to_partial_guild(&ctx).await?.system_channel_id {
                Some(v) => v,
                None => return Err(serenity::Error::Other("no system messages channel")),
            };
            let mut data = String::new();
            writeln!(
                &mut data,
                "JOIN     <{}> @{} ({})",
                new_member
                    .joined_at
                    .unwrap_or_else(chrono::Utc::now)
                    .to_rfc3339(),
                new_member.user.id,
                escape(&new_member.user.tag())
            )?;

            let mut log = self.get_log_file(&ctx, channel).await?;
            log.write_all(data.as_bytes()).await?;
            Ok(())
        }
        .await;
    }

    async fn guild_member_removal(
        &self,
        ctx: Context,
        server: GuildId,
        user: User,
        _member_data_if_available: Option<Member>,
    ) {
        let _: Result<()> = async {
            let channel = match server.to_partial_guild(&ctx).await?.system_channel_id {
                Some(v) => v,
                None => return Err(serenity::Error::Other("no system messages channel")),
            };
            let mut data = String::new();
            writeln!(
                &mut data,
                "LEAVE    <{}> @{} ({})",
                chrono::Utc::now().to_rfc3339(),
                user.id,
                escape(&user.tag())
            )?;

            let mut log = self.get_log_file(&ctx, channel).await?;
            log.write_all(data.as_bytes()).await?;
            Ok(())
        }
        .await;
    }
}

fn escape(name: &str) -> String {
    let mut result = String::new();
    for c in name.chars() {
        if c == '\\' {
            result.push_str("\\\\");
        } else if c.is_ascii_graphic() && c != '/' || c == ' ' || c.is_alphanumeric() {
            result.push(c);
        } else {
            result.extend(c.escape_unicode());
        }
    }
    result
}

static NITRO_PATTERN: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r"\bnitro\b").case_insensitive(true).build().unwrap()
});

async fn is_nitro_scam(content: &str) -> bool {
    if !content.contains("@everyone") {
        return false;
    }
    if !NITRO_PATTERN.is_match(content) {
        return false
    }
    true
}
