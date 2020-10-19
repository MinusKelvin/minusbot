use serenity::prelude::*;
use serenity::framework::standard::macros::{ hook };
use serenity::model::channel::Message;
use serenity::http::AttachmentType;
use regex::Regex;
use fumen::Fumen;
use lazy_static::lazy_static;

#[hook]
pub async fn normal_message(ctx: &Context, msg: &Message) {
    if let Some(fumen) = extract_fumen(&msg.content).await {
        let gif = tokio::task::spawn_blocking(|| render_fumen(fumen)).await.unwrap().unwrap();
        msg.channel_id.send_files(&ctx.http, vec![AttachmentType::Bytes {
            data: gif.into(),
            filename: "fumen.gif".into()
        }], |f| f).await.unwrap();
    }
}

async fn extract_fumen(text: &str) -> Option<Fumen> {
    lazy_static! {
        static ref FUMEN_DATA: Regex = Regex::new(r"v115@[^ ]+").unwrap();
        static ref TINYURL: Regex = Regex::new(r"(https?://)?tinyurl.com/[0-9a-zA-Z\-]+").unwrap();
    }

    if let Some(data) = FUMEN_DATA.find(text) {
        Fumen::decode(data.as_str()).ok()
    } else if let Some(url) = TINYURL.find(text) {
        let url = if url.as_str().starts_with("http") {
            url.as_str().to_string()
        } else {
            "https://".to_string() + url.as_str()
        };
        let response = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build().ok()?
            .get(&url)
            .send().await.ok()?;
        let target = response.headers().get("Location")?;
        FUMEN_DATA.find(target.to_str().ok()?)
            .and_then(|data| Fumen::decode(data.as_str()).ok())
    } else {
        None
    }
}

fn render_fumen(fumen: Fumen) -> Result<Vec<u8>, gif::EncodingError> {
    const GLOBAL_PALETTE: &'static [u8] = &[
        0x40, 0x40, 0x40,
        0x00, 0xFF, 0xFF,
        0xFF, 0x80, 0x00,
        0xFF, 0xFF, 0x00,
        0xFF, 0x00, 0x00,
        0x80, 0x00, 0xFF,
        0x00, 0x20, 0xFF,
        0x00, 0xFF, 0xFF,
        0x80, 0x80, 0x80,
        0x10, 0x10, 0x10
    ];
    const BLOCK_SIZE: usize = 16;

    let has_garbage_row = fumen.pages.iter()
        .any(|p| p.garbage_row != [fumen::CellColor::Empty; 10]);
    let height = has_garbage_row as usize + fumen.pages.iter()
        .map(|p| p.field.iter()
            .enumerate()
            .rfind(|(_,&r)| r != [fumen::CellColor::Empty; 10])
            .map(|(i,_)| i)
            .unwrap_or(0)
            .max(p.piece
                .map(to_libtetris)
                .and_then(|p| p.cells().iter()
                    .map(|&(_,y)| y as usize)
                    .max())
                .unwrap_or(0)
            )
        ).max().unwrap() + 1;
    let gif_height = height*BLOCK_SIZE + 2*(has_garbage_row as usize);
    let mut gif_data = vec![];
    let mut writer = gif::Encoder::new(
        &mut gif_data, BLOCK_SIZE as u16 * 10, gif_height as u16, GLOBAL_PALETTE
    )?;
    writer.set_repeat(gif::Repeat::Infinite)?;

    for page in fumen.pages {
        let mut buf = vec![0; BLOCK_SIZE as usize * 10 * gif_height];
        let mut fill_tile = |x: usize, y: i32, color: fumen::CellColor| {
            let tp = BLOCK_SIZE*x + BLOCK_SIZE*BLOCK_SIZE*10*(
                (height as i32 - y - 1 - has_garbage_row as i32) as usize
            );
            for iy in 0..BLOCK_SIZE {
                for ix in 0..BLOCK_SIZE {
                    let i = tp + iy*10*BLOCK_SIZE + ix;
                    if has_garbage_row && y == -1 {
                        if iy < 2 {
                            buf[i] = 9;
                        }
                        buf[i + 2*10*BLOCK_SIZE] = color as u8;
                    } else {
                        buf[i] = color as u8;
                    }
                }
            }
        };
        for y in 0..height - has_garbage_row as usize {
            for x in 0..10 {
                fill_tile(x, y as i32, page.field[y][x]);
            }
        }
        if has_garbage_row {
            for x in 0..10 {
                fill_tile(x, -1, page.garbage_row[x]);
            }
        }
        if let Some(piece) = page.piece {
            for &(x, y) in &to_libtetris(piece).cells() {
                fill_tile(x as usize, y, piece.kind.into());
            }
        }
        writer.write_frame(&gif::Frame {
            delay: 100,
            width: 160,
            height: gif_height as u16,
            buffer: buf.into(),
            ..Default::default()
        })?
    }

    drop(writer);

    Ok(gif_data)
}

fn to_libtetris(p: fumen::Piece) -> libtetris::FallingPiece {
    libtetris::FallingPiece {
        tspin: libtetris::TspinStatus::None,
        x: p.x as i32,
        y: p.y as i32,
        kind: libtetris::PieceState(match p.kind {
            fumen::PieceType::I => libtetris::Piece::I,
            fumen::PieceType::O => libtetris::Piece::O,
            fumen::PieceType::T => libtetris::Piece::T,
            fumen::PieceType::S => libtetris::Piece::S,
            fumen::PieceType::Z => libtetris::Piece::Z,
            fumen::PieceType::L => libtetris::Piece::L,
            fumen::PieceType::J => libtetris::Piece::J,
        }, match p.rotation {
            fumen::RotationState::North => libtetris::RotationState::North,
            fumen::RotationState::South => libtetris::RotationState::South,
            fumen::RotationState::East => libtetris::RotationState::East,
            fumen::RotationState::West => libtetris::RotationState::West,
        })
    }
}