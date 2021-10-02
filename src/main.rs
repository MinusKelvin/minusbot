use serenity::prelude::*;
use serenity::framework::standard::{ StandardFramework };
use serenity::client::bridge::gateway::GatewayIntents;

mod tetris;
mod logger;

#[tokio::main]
async fn main() {
    let framework = StandardFramework::new()
        .configure(|c| c
            .prefix("-")
        )
        .normal_message(tetris::normal_message)
        .group(&tetris::TETRIS_GROUP);

    let mut client = Client::builder(std::fs::read_to_string(".token").unwrap().trim())
        .intents(GatewayIntents::GUILD_MEMBERS | GatewayIntents::GUILD_MESSAGES | GatewayIntents::DIRECT_MESSAGES)
        .framework(framework)
        .event_handler(logger::Logger::new())
        .await
        .unwrap();
    
    if let Err(e) = client.start().await {
        println!("{:?}", e);
    }
}
