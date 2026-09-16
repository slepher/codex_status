use tauri::image::Image;

pub const SIZE: u32 = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Ok,
    Stale,
    Error,
    NoData,
}

impl State {
    fn color(self) -> [u8; 4] {
        match self {
            State::Ok => [0x22, 0xA5, 0x5E, 0xFF],
            State::Stale => [0xE6, 0xA7, 0x00, 0xFF],
            State::Error => [0xD9, 0x3A, 0x3A, 0xFF],
            State::NoData => [0x8A, 0x8A, 0x8A, 0xFF],
        }
    }
}

fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        _ => [0; 5],
    }
}

fn set_pixel(px: &mut [u8], x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
        return;
    }
    let i = ((y as u32 * SIZE + x as u32) * 4) as usize;
    px[i..i + 4].copy_from_slice(&c);
}

fn fill_rect(px: &mut [u8], x: i32, y: i32, w: i32, h: i32, c: [u8; 4]) {
    for yy in y..y + h {
        for xx in x..x + w {
            set_pixel(px, xx, yy, c);
        }
    }
}

fn text_width(text: &str, scale: i32) -> i32 {
    let len = text.chars().count() as i32;
    if len == 0 {
        return 0;
    }
    (len * 4 - 1) * scale
}

fn draw_text(px: &mut [u8], text: &str, x: i32, y: i32, scale: i32, c: [u8; 4]) {
    let mut cursor = x;
    for ch in text.chars() {
        let rows = glyph(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) == 0 {
                    continue;
                }
                fill_rect(
                    px,
                    cursor + col as i32 * scale,
                    y + row as i32 * scale,
                    scale,
                    scale,
                    c,
                );
            }
        }
        cursor += 4 * scale;
    }
}

/// Draw the tray icon: status-colored plate with the percent value as digits.
pub fn render(percent: Option<i32>, state: State) -> Image<'static> {
    let mut px = vec![0u8; (SIZE * SIZE * 4) as usize];
    let plate = state.color();
    fill_rect(&mut px, 1, 1, SIZE as i32 - 2, SIZE as i32 - 2, plate);
    // soft corners
    for (x, y) in [(1, 1), (SIZE as i32 - 2, 1), (1, SIZE as i32 - 2), (SIZE as i32 - 2, SIZE as i32 - 2)] {
        set_pixel(&mut px, x, y, [0, 0, 0, 0]);
    }
    let text = match percent {
        Some(p) => p.clamp(0, 100).to_string(),
        None => "--".to_string(),
    };
    let scale = if text.len() >= 3 { 2 } else { 3 };
    let w = text_width(&text, scale);
    let x = (SIZE as i32 - w) / 2;
    let y = (SIZE as i32 - 5 * scale) / 2;
    draw_text(&mut px, &text, x, y, scale, [0xFF, 0xFF, 0xFF, 0xFF]);
    Image::new_owned(px, SIZE, SIZE)
}
