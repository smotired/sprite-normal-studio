# Sprite Normal Studio
Desktop application for creating normal maps for hi-res spritesheets.

## Abstract

Every normal map creator tool I have been able to find is mainly for textures on 3D models, which is a vastly different experience than for the hand-drawn sprites I'm using in my game Throwing Hands. A texture is one big complicated image, but for hand drawn sprites, you could have tens or even hundreds of very similar images. Any software that relies on direct pixel manipulation (even with shape brushes like some software has) is therefore not viable for this application.

Sprite Normal Studio will instead use an approach based on bezier paths. A sprite will be made up of multiple paths, called zones, for each surface of the image. Each zone will also have shapes within it, which are primitive objects that loosely define the surface of the zone. Normal maps are contructed from blending between the shapes in each zone. The main idea is that you can copy the zones and shapes between sprites in an animation, and just warp the shapes as the sprite changes, which should be much quicker and more consistent than traditional approaches.

Basically, other software is to Photoshop what this is to Illustrator.

## Goals

- Speed up development of Throwing Hands.
- Learn how to use Rust effectively.
- Learn a more portable and more knowledge-transferrable way of working with the GPU than CUDA.
- Finish and release as a commercial product.

## Development plan

### Phase I
*9/21/26 - 9/25/26*

```
[x] Research Rust libraries for UI and for GPU usage
[x] Get a basic UI to open
[x] Get a basic compute shader running with the hello-world fragment texture
[x] Render the shader's output texture directly to the window
[x] Ensure quick shader performance, check by resizing and everything
```

### Phase II
*9/25/26 - current*

```
[x] Allow selecting files for sprite and normal map
[ ] Previews for sprite and normal map
[ ] Create a default normal map in memory if selected path does not exist
[ ] Pass sprite and normal map to compute shader and render the sprite
[ ] Add a light source to shader as if it had a flat normal map
[ ] Render with the normal map
[ ] Add camera controls
[ ] Add controls for the light source
```

### Phase III

```
[ ] Create a Zone struct
[ ] Render Zone boundaries
[ ] Controls to show/hide zone paths
[ ] Assign pixels to zones in compute shader
[ ] Allow creating zones with bezier paths
[ ] Modify zone paths, at path and vertex level, including deletion and
    duplication
[ ] Joining zone paths at certain vertices, which should update both paths
    when manipulated
[ ] Selecting and manipulating multiple zones/vertices at once
[ ] Set base normal for a zone, which will be applied after all shapes.
    Set up compute shader pipeline for this.
```

### Phase IV

```
[ ] Undo/redo with history. Use a stack like Unity's Undo class. Look for an
    external crate first.
[ ] Saving workspace files (with path to sprite and normal map, all shapes,
    zones, and lights, but not camera)
[ ] Loading workspace files
[ ] Exporting normal map (Ctrl+E exports to path with confirmation if it
    would overwrite. Ctrl+Shift+E opens dialog.)
[ ] Having multiple workspace files open
[ ] Copy/pasting zones between workspace files
```

### Phase V

```
[ ] Shape trait, and basic Point shape that acts like the top of a cone
[ ] Shapes assigned to zones
[ ] Render shapes themselves in viewport
[ ] Controls to show and hide shapes (hiding zone paths always hides shapes)
[ ] Shape management tools
[ ] Render shape normals in compute shader
[ ] Blend normals between multiple shapes based on distance
[ ] Line shape with an outer radius
[ ] Circle shape with an inner (flat) and outer (dome) radius
[ ] Triangle shape with an outer radius
[ ] Allow lines to share control points
[ ] Allow triangles to share control points
[ ] Manipulating a zone should also manipulate its shapes
```

### Interlude

```
[ ] Vital bug-fixes/polish
[ ] Use during development of Throwing Hands, until that project is finally
    done and released and I can move on with my life.
```

### Phase VI

Finish, polish, and deploy as a commercial product.
Will add steps to this section as I think of them.

```
[ ] Better UI - final icons, repeatable welcome tutorial, etc.
[ ] Full testing suite
[ ] CI and automatic deployment to Windows, MacOS, Linux
[ ] Bug reporting and support system other than GitHub issues
[ ] Trial version (maybe where you just can't save/export)
[ ] Legal/licensing
[ ] Website
[ ] Look into a web deployment (or even just demo) because eframe and wgpu
    should both be web compatible
[ ] User testing
[ ] Marketing
[ ] Launch
```