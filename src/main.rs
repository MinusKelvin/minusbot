use serenity::prelude::*;
use serenity::framework::standard::{ StandardFramework };

mod tetris;

#[tokio::main]
async fn main() {
    let framework = StandardFramework::new()
        .configure(|c| c
            .prefix("-")
        )
        .normal_message(tetris::normal_message)
        .group(&tetris::TETRIS_GROUP);

    let mut client = Client::new(std::fs::read_to_string(".token").unwrap().trim())
        .framework(framework)
        .await
        .unwrap();
    
    if let Err(e) = client.start().await {
        println!("{:?}", e);
    }
}
