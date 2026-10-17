/* preview.wgsl
 * Draws one lit sprite of the spritesheet for the animation preview,
 * centered and cropped to the sprite, with a thumb for its light.
 * Separate from the viewport shader, which draws far more.
 *******************************************************************/

@group(0) @binding(0) var output: texture_storage_2d<rgba8unorm, write>;

struct PreviewParams {
    // Top left pixel and size of the sprite in the spritesheet. Everything outside it is drawn as background.
    sprite_min: vec2<f32>,
    sprite_size: vec2<f32>,

    // Position of the light relative to the top left of the sprite
    light_offset: vec2<f32>,

    // Sprite pixels per screen point, inverted. 1/2 means each sprite pixel takes up 2 screen points each direction.
    inv_scale: f32,

    // Physical pixels per screen point
    pixels_per_point: f32,

    // Height of the light, in sprite pixels
    light_height: f32,

    // Light color, packed as 0x00BBGGRR
    light_color: u32,

    // 00: Shade with the normal map. If lighting is off, draws the normal map instead of the sprite.
    // 01: Light the sprite
    // 02: The light is being dragged, so its halo is drawn
    flags: u32,

    _padding: u32,
}
@group(0) @binding(1) var<uniform> params: PreviewParams;

@group(0) @binding(2) var sprite: texture_2d<f32>;
@group(0) @binding(3) var normal: texture_2d<f32>;

// Same as the viewport, so the preview looks like the sprite does there
const GRAY_OOB = 0.2;           // Gray color outside the sprite
const GRAY_BACKGROUND = 0.25;   // Gray color for the transparent areas of the sprite
const AMBIENT_LIGHT = 0.2;      // How much of the light is replaced with ambient white light

const LIGHT_RADIUS = 6.0;       // Radius of the light thumb, in screen points
const LIGHT_OUTLINE = 1.0;      // Width of the light thumb's outline, in screen points
const HALO_WIDTH = 1.0;         // Width of the halo showing the light's height, in screen points

fn flag(id: u32) -> bool {
    return (params.flags & (1u << id)) != 0u;
}

// The lit color of the sprite at a pixel of the spritesheet
fn sprite_color(pos: vec2<f32>) -> vec3<f32> {
    let pxl = vec2<i32>(floor(pos));
    let background = vec3<f32>(1, 1, 1) * GRAY_BACKGROUND;

    // Normal map instead of the sprite
    if (flag(0) && !flag(1)) {
        let norm_color = textureLoad(normal, pxl, 0);
        return mix(background, norm_color.rgb, norm_color.a);
    }

    let color = textureLoad(sprite, pxl, 0);
    if (!flag(1)) {
        return mix(background, color.rgb, color.a);
    }

    var norm = vec3<f32>(0, 0, 1);
    if (flag(0)) {
        norm = normalize(textureLoad(normal, pxl, 0).rgb * 2.0 - 1.0);
        norm.y = -norm.y; // the normal map is y-up but the sprite is y-down
    }

    let light_pos = vec3<f32>(params.sprite_min + params.light_offset, params.light_height);
    let to_light = light_pos - vec3<f32>(pos, 0);
    let light_distance = length(to_light);
    let diffuse = max(dot(norm, normalize(to_light)), 0.0);
    let attenuation = (light_pos.z * light_pos.z) / max(light_distance * light_distance, 1.0);
    let light_color = unpack4x8unorm(params.light_color).rgb * (1.0 - AMBIENT_LIGHT);

    let reflected = color.rgb * light_color * diffuse * attenuation;
    let ambient = color.rgb * AMBIENT_LIGHT;
    return mix(background, reflected + ambient, color.a);
}

// Mix a color in where a screen space distance is within a ring of a radius and half width, softened over one point
fn ring(color: vec3<f32>, ring_color: vec3<f32>, distance: f32, radius: f32, half_width: f32) -> vec3<f32> {
    let coverage = clamp(half_width + 0.5 - abs(distance - radius), 0.0, 1.0);
    return mix(color, ring_color, coverage);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Screen position in points, from the center, and the sprite pixel there. The sprite is always centered.
    let screen = (vec2<f32>(id.xy) + 0.5 - vec2<f32>(size) * 0.5) / params.pixels_per_point;
    let center = params.sprite_min + params.sprite_size * 0.5;
    let pos = center + screen * params.inv_scale;

    var color = vec3<f32>(1, 1, 1) * GRAY_OOB;
    let local = pos - params.sprite_min;
    if (local.x >= 0.0 && local.y >= 0.0 && local.x < params.sprite_size.x && local.y < params.sprite_size.y) {
        color = sprite_color(pos);
    }

    // The light's thumb, a fixed size on screen, and its halo while dragging, whose radius is the light's height
    let light_screen = (params.sprite_min + params.light_offset - center) / params.inv_scale;
    let light_distance = length(screen - light_screen);
    if (flag(2)) {
        color = ring(color, vec3<f32>(1, 1, 1), light_distance, params.light_height / params.inv_scale, HALO_WIDTH * 0.5);
    }
    let fill = clamp(LIGHT_RADIUS - LIGHT_OUTLINE + 0.5 - light_distance, 0.0, 1.0);
    color = mix(color, vec3<f32>(1, 1, 1), fill * 0.8);
    color = ring(color, vec3<f32>(0, 0, 0), light_distance, LIGHT_RADIUS - LIGHT_OUTLINE * 0.5, LIGHT_OUTLINE * 0.5);

    textureStore(output, id.xy, vec4<f32>(color, 1.0));
}
