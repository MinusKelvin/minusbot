use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serenity::client::bridge::gateway::GatewayIntents;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandResult, StandardFramework};
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, GuildId, RoleId};
use serenity::prelude::*;
use tokio::sync::{MappedMutexGuard, MutexGuard};

mod logger;
mod tetris;

#[tokio::main]
async fn main() {
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("-"))
        .normal_message(tetris::normal_message)
        .group(&tetris::TETRIS_GROUP)
        .group(&CONFIGURATION_GROUP);

    let mut client = Client::builder(std::fs::read_to_string(".token").unwrap().trim())
        .intents(
            GatewayIntents::GUILD_MEMBERS
                | GatewayIntents::GUILD_MESSAGES
                | GatewayIntents::DIRECT_MESSAGES,
        )
        .framework(framework)
        .event_handler(logger::Logger::new())
        .await
        .unwrap();

    if let Err(e) = client.start().await {
        println!("{:?}", e);
    }
}

static CONFIGURATION: Lazy<Mutex<HashMap<GuildId, Config>>> =
    Lazy::new(|| match std::fs::File::open("server-config.json") {
        Ok(f) => {
            let reader = std::io::BufReader::new(f);
            let map = serde_json::from_reader(reader).unwrap_or_else(|_| HashMap::new());
            Mutex::new(map)
        }
        Err(_) => Mutex::default(),
    });

async fn config(guild: GuildId) -> MappedMutexGuard<'static, Config> {
    MutexGuard::map(CONFIGURATION.lock().await, |map| {
        map.entry(guild).or_insert_with(Default::default)
    })
}

async fn save_config() {
    let guard = CONFIGURATION.lock().await;
    let f = std::fs::File::create("server-config.json").unwrap();
    let writer = std::io::BufWriter::new(f);
    serde_json::to_writer(writer, &*guard).unwrap();
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    muted_role: RoleId,
    admin_channel: ChannelId,
}

#[group]
#[only_in(guilds)]
#[required_permissions(ADMINISTRATOR)]
#[commands(set_muted_role, set_admin_channel)]
pub struct Configuration;

#[command]
async fn set_muted_role(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let role_name = match args.trimmed().current() {
        Some(v) => v,
        None => {
            msg.channel_id
                .say(&ctx, "Please provide a role name to set")
                .await?;
            return Ok(());
        }
    };
    let guild = match msg.guild_id {
        Some(v) => v,
        None => {
            msg.channel_id
                .say(
                    &ctx,
                    "Could not find what server this is. I am very confused.",
                )
                .await?;
            return Ok(());
        }
    };
    let role_id = match guild
        .roles(&ctx)
        .await?
        .into_iter()
        .find(|(_, r)| r.name == role_name)
    {
        Some(v) => v.0,
        None => {
            msg.channel_id
                .say(&ctx, "That doesn't appear to be a valid role")
                .await?;
            return Ok(());
        }
    };

    let mut config = config(guild).await;
    config.muted_role = role_id;
    drop(config);

    save_config().await;

    Ok(())
}

#[command]
async fn set_admin_channel(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let guild = match msg.guild_id {
        Some(v) => v,
        None => {
            msg.channel_id
                .say(
                    &ctx,
                    "Could not find what server this is. I am very confused.",
                )
                .await?;
            return Ok(());
        }
    };

    let mut config = config(guild).await;
    config.admin_channel = msg.channel_id;
    drop(config);

    save_config().await;

    Ok(())
}
