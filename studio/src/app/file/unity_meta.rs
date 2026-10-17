use std::path::Path;

/// Sprite mode of a texture sliced into multiple sprites, in Unity's TextureImporter
const SPRITE_MODE_MULTIPLE: &str = "2";

/// Find the size of the sprites a spritesheet is sliced into in Unity, from the .meta file next to it (i.e. hero.png.meta).
/// Only textures in Multiple sprite mode are sliced, and every sprite is assumed to be the size of the first one.
/// Returns None if there is no .meta file, or it doesn't slice the spritesheet.
pub fn sprite_size(image: &Path) -> Option<(u32, u32)> {
    let mut meta = image.as_os_str().to_owned();
    meta.push(".meta");
    parse_sprite_size(&std::fs::read_to_string(meta).ok()?)
}

/// Find the size of the first sprite in the text of a Unity .meta file, if its sprite mode is Multiple.
/// Unity writes .meta files as YAML, but only a few keys are needed, so it's read line by line by indentation:
///
/// ```yaml
/// TextureImporter:
///   spriteMode: 2
///   spriteSheet:
///     sprites:
///     - serializedVersion: 2
///       name: hero_0
///       rect:
///         serializedVersion: 2
///         x: 0
///         y: 64
///         width: 32
///         height: 48
/// ```
fn parse_sprite_size(meta: &str) -> Option<(u32, u32)> {
    let lines: Vec<(usize, &str)> = meta.lines()
        .map(|line| (line.len() - line.trim_start().len(), line.trim()))
        .filter(|(_, line)| !line.is_empty())
        .collect();

    if !lines.iter().any(|&(_, line)| value(line, "spriteMode") == Some(SPRITE_MODE_MULTIPLE)) { return None; }

    // The list of sprites. Its items may be at the same indentation as the key, starting with a dash.
    let start = lines.iter().position(|&(_, line)| line == "sprites:")?;
    let sprites_indent = lines[start].0;
    let in_sprites = |&(indent, line): &(usize, &str)| indent > sprites_indent || (indent == sprites_indent && line.starts_with('-'));
    let sprites: Vec<(usize, &str)> = lines[start + 1..].iter().copied().take_while(in_sprites).collect();

    // The first sprite's rect, and the width and height inside it
    let rect = sprites.iter().position(|&(_, line)| line.trim_start_matches("- ") == "rect:")?;
    let rect_indent = sprites[rect].0;
    let fields: Vec<&str> = sprites[rect + 1..].iter()
        .take_while(|&&(indent, _)| indent > rect_indent)
        .map(|&(_, line)| line)
        .collect();
    let field = |key: &str| fields.iter().find_map(|line| value(line, key)).and_then(|value| value.parse::<f32>().ok());

    let (width, height) = (field("width")?, field("height")?);
    (width >= 1.0 && height >= 1.0).then(|| (width.round() as u32, height.round() as u32))
}

/// The value of a `key: value` line, if it is for that key
fn value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (line_key, value) = line.trim_start_matches("- ").split_once(':')?;
    (line_key.trim() == key).then(|| value.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A .meta file with the given sprite mode and the sprites section
    fn meta(mode: u32, sprites: &str) -> String {
        format!("fileFormatVersion: 2
guid: 0123456789abcdef
TextureImporter:
  serializedVersion: 12
  spriteMode: {mode}
  spriteExtrude: 1
  spriteMeshType: 1
  spritePixelsToUnits: 100
  spriteSheet:
    serializedVersion: 2
{sprites}
    outline: []
    physicsShape: []
  spritePackingTag:
")
    }

    const SPRITES: &str = "    sprites:
    - serializedVersion: 2
      name: hero_0
      rect:
        serializedVersion: 2
        x: 0
        y: 64
        width: 32
        height: 48
      alignment: 0
      pivot: {x: 0.5, y: 0.5}
    - serializedVersion: 2
      name: hero_1
      rect:
        serializedVersion: 2
        x: 32
        y: 64
        width: 16
        height: 16";

    /// Multiple mode gives the size of the first sprite
    #[test]
    fn multiple() {
        assert_eq!(parse_sprite_size(&meta(2, SPRITES)), Some((32, 48)));
    }

    /// Windows line endings are read the same
    #[test]
    fn crlf() {
        assert_eq!(parse_sprite_size(&meta(2, SPRITES).replace('\n', "\r\n")), Some((32, 48)));
    }

    /// Single mode isn't sliced, even if old sprites are left in the file
    #[test]
    fn single() {
        assert_eq!(parse_sprite_size(&meta(1, SPRITES)), None);
    }

    /// Multiple mode with no sprites isn't sliced yet
    #[test]
    fn no_sprites() {
        assert_eq!(parse_sprite_size(&meta(2, "    sprites: []")), None);
    }

    /// A rect after the sprites list, like the one in another section, isn't used
    #[test]
    fn rect_outside_sprites() {
        let sprites = "    sprites: []\n  other:\n    rect:\n      width: 8\n      height: 8";
        assert_eq!(parse_sprite_size(&meta(2, sprites)), None);
    }

    /// Files that aren't .meta files, or are missing values, give nothing
    #[test]
    fn malformed() {
        assert_eq!(parse_sprite_size(""), None);
        assert_eq!(parse_sprite_size("not: yaml: at all"), None);
        assert_eq!(parse_sprite_size(&meta(2, &SPRITES.replace("height: 48", "height: lots"))), None);
        assert_eq!(parse_sprite_size(&meta(2, &SPRITES.replace("width: 32", "width: 0"))), None);
    }

    /// A missing .meta file gives nothing
    #[test]
    fn missing_file() {
        assert_eq!(sprite_size(Path::new("definitely/not/a/real/hero.png")), None);
    }
}
