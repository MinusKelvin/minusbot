use serenity::prelude::*;
use serenity::framework::standard::macros::{ hook, group, command };
use serenity::framework::standard::{ CommandResult, Args };
use serenity::model::channel::Message;
use serenity::http::AttachmentType;
use regex::Regex;
use fumen::Fumen;
use lazy_static::lazy_static;
use libtetris::{ Board };

#[group]
#[commands(cold_clear_analysis)]
pub struct Tetris;

#[command]
#[aliases("cc")]
async fn cold_clear_analysis(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let fumen_data = match args.trimmed().current() {
        Some(data) => data,
        None => {
            msg.channel_id.say(&ctx.http, "Please pass a fumen to analyse").await?;
            return Ok(())
        }
    };
    let (fumen, options) = match extract_fumen(fumen_data).await {
        Some(data) => data,
        None => {
            msg.channel_id.say(&ctx.http, "Invalid fumen").await?;
            return Ok(())
        }
    };
    let options = options.to_owned();
    if fumen.pages.len() != 1 {
        msg.channel_id.say(&ctx.http, "Fumen should have 1 page and a queue comment.").await?;
        return Ok(())
    }
    let page = &fumen.pages[0];
    lazy_static! {
        static ref QUEUE_SELECTOR: Regex = Regex::new(
            r"^#Q=\[([IOTJLSZ]?)\]\(([IOTJLSZ])\)([IOTJLSZ]*)$"
        ).unwrap();
    }
    if let Some(caps) = page.comment.as_ref().and_then(|c| QUEUE_SELECTOR.captures(c)) {
        let hold = caps.get(1).unwrap();
        let hold = hold.as_str().chars().next().and_then(from_char);
        let current = caps.get(2).unwrap();
        let next = caps.get(3).unwrap();
        let mut field = [[false; 10]; 40];
        for y in 0..23 {
            for x in 0..10 {
                field[y][x] = page.field[y][x] != fumen::CellColor::Empty;
            }
        }
        if field.iter().any(|&r| r == [true; 10]) {
            msg.channel_id.say(&ctx.http, "Fumen contains a complete row.").await?;
            return Ok(())
        }
        let mut board = Board::new_with_state(field, Default::default(), hold, false, 0);
        board.add_next_piece(current.as_str().chars().next().and_then(from_char).unwrap());
        for c in next.as_str().chars() {
            board.add_next_piece(from_char(c).unwrap());
        }

        let count = (board.next_queue().count() + (hold.is_some() as usize) - 1).min(40);

        msg.channel_id.broadcast_typing(&ctx.http).await?;

        println!("Running Cold Clear...");

        let cc = cold_clear::Interface::launch(board, cold_clear::Options {
            speculate: false,
            pcloop: None,
            ..Default::default()
        }, cold_clear::evaluation::Standard::default(), None);

        let mut fumen = Fumen::default();
        let first_page = fumen.add_page();
        first_page.field = page.field;

        for _ in 0..count {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            cc.suggest_next_move(0);
            tokio::task::yield_now().await;
            if let Some((mv, info)) = cc.block_next_move() {
                cc.play_next_move(mv.expected_location);
                let page = fumen.add_page();
                page.piece = Some(to_fumen(mv.expected_location));
                if let cold_clear::Info::Normal(info) = info {
                    page.comment = Some(format!("{}n, {}d", info.nodes, info.depth));
                }
            } else {
                break;
            }
        }

        let gif = tokio::task::spawn_blocking(
            move || render_fumen(fumen, &options)
        ).await.unwrap().unwrap();
        msg.channel_id.send_files(&ctx.http, vec![AttachmentType::Bytes {
            data: gif.into(),
            filename: "fumen.gif".into()
        }], |f| f).await.unwrap();

        Ok(())
    } else {
        msg.channel_id.say(&ctx.http, "Fumen should have 1 page and a queue comment.").await?;
        Ok(())
    }
}

#[hook]
pub async fn normal_message(ctx: &Context, msg: &Message) {
    if msg.content.starts_with('-') {
        return
    }
    if let Some((fumen, options)) = extract_fumen(&msg.content).await {
        let options = options.to_owned();
        let gif = tokio::task::spawn_blocking(
            move || render_fumen(fumen, &options)
        ).await.unwrap().unwrap();
        msg.channel_id.send_files(&ctx.http, vec![AttachmentType::Bytes {
            data: gif.into(),
            filename: "fumen.gif".into()
        }], |f| f).await.unwrap();
    }
}

async fn extract_fumen(text: &str) -> Option<(Fumen, &str)> {
    lazy_static! {
        static ref FUMEN_DATA: Regex = Regex::new(r"(v115@[a-zA-Z0-9+/?]+)(#[^ ]+)?").unwrap();
        static ref TINYURL: Regex = Regex::new(
            r"((?:https?://)?tinyurl.com/[0-9a-zA-Z\-]+)(#[^ ]+)?"
        ).unwrap();
    }

    if let Some(caps) = FUMEN_DATA.captures(text) {
        let data = caps.get(1).unwrap().as_str();
        println!("Found fumen {}", data);
        Fumen::decode(data).map_err(|e| {
            println!("Failed to decode fumen: {}", e);
            e
        }).ok().map(|f| (f, caps.get(2).map(|m| m.as_str()).unwrap_or("")))
    } else if let Some(caps) = TINYURL.captures(text) {
        let url = caps.get(1).unwrap().as_str();
        let url = if url.starts_with("http") {
            url.to_string()
        } else {
            "https://".to_string() + url
        };
        let response = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build().ok()?
            .get(&url)
            .send().await.ok()?;
        let target = response.headers().get("Location")?;
        println!("Found fumen {} in tinyurl", target.to_str().unwrap());
        FUMEN_DATA.captures(target.to_str().ok()?)
            .and_then(|caps| Fumen::decode(caps.get(1).unwrap().as_str())
                .map_err(|e| {
                    println!("Failed to decode fumen: {}", e);
                    e
                }).ok()
            ).map(|f| (f, caps.get(2).map(|m| m.as_str()).unwrap_or("")))
    } else {
        None
    }
}

fn render_fumen(fumen: Fumen, options: &str) -> Result<Vec<u8>, gif::EncodingError> {
    lazy_static! {
        static ref EXTRACT_OPTIONS: Regex = Regex::new(
            r"([\w._]+)=([\w._]+)"
        ).unwrap();
    }

    const GLOBAL_PALETTE: &'static [u8] = &[
        0x40, 0x40, 0x40,
        0x00, 0xFF, 0xFF,
        0xFF, 0x80, 0x00,
        0xFF, 0xFF, 0x00,
        0xFF, 0x00, 0x00,
        0x80, 0x00, 0xFF,
        0x00, 0x20, 0xFF,
        0x00, 0xFF, 0x00,
        0x80, 0x80, 0x80,
        0x10, 0x10, 0x10
    ];
    const BLOCK_SIZE: usize = 24;

    let mut speed = 1.0f64;
    for caps in EXTRACT_OPTIONS.captures_iter(options) {
        let key = caps.get(1).unwrap().as_str();
        let value = caps.get(2).unwrap().as_str();
        match key {
            "speed" => if let Ok(s) = value.parse() {
                speed = s;
            }
            _ => {}
        }
    }

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
            delay: (50.0 / speed).round() as u16,
            width: BLOCK_SIZE as u16 * 10,
            height: gif_height as u16,
            buffer: buf.into(),
            ..Default::default()
        })?
    }

    drop(writer);

    println!("gif is {} bytes large", gif_data.len());

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

fn to_fumen(p: libtetris::FallingPiece) -> fumen::Piece {
    fumen::Piece {
        x: p.x as u32,
        y: p.y as u32,
        kind: match p.kind.0 {
            libtetris::Piece::I => fumen::PieceType::I,
            libtetris::Piece::O => fumen::PieceType::O,
            libtetris::Piece::T => fumen::PieceType::T,
            libtetris::Piece::S => fumen::PieceType::S,
            libtetris::Piece::Z => fumen::PieceType::Z,
            libtetris::Piece::L => fumen::PieceType::L,
            libtetris::Piece::J => fumen::PieceType::J,
        },
        rotation: match p.kind.1 {
            libtetris::RotationState::North => fumen::RotationState::North,
            libtetris::RotationState::South => fumen::RotationState::South,
            libtetris::RotationState::East => fumen::RotationState::East,
            libtetris::RotationState::West => fumen::RotationState::West,
        }
    }
}

fn from_char(c: char) -> Option<libtetris::Piece> {
    match c {
        'I' => Some(libtetris::Piece::I),
        'O' => Some(libtetris::Piece::O),
        'T' => Some(libtetris::Piece::T),
        'L' => Some(libtetris::Piece::L),
        'J' => Some(libtetris::Piece::J),
        'S' => Some(libtetris::Piece::S),
        'Z' => Some(libtetris::Piece::Z),
        _ => None
    }
}