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

### Phase I - Initial
*9/21/26 - 9/25/26*

```
[x] Research Rust libraries for UI and for GPU usage
[x] Get a basic UI to open
[x] Get a basic compute shader running with the hello-world fragment texture
[x] Render the shader's output texture directly to the window
[x] Ensure quick shader performance, check by resizing and everything
```

### Phase II - Basic Rendering
*9/25/26 - 9/29/26*

```
[x] Allow selecting files for sprite and normal map
[x] Previews for sprite and normal map
[x] Create a default normal map in memory if selected path does not exist
[x] Pass sprite and normal map to compute shader and render the sprite
[x] Add a light source to shader as if it had a flat normal map
[x] Render with the normal map
[x] Add camera controls
[x] Add controls for the light source and a widget to show where it is.
    This will require setting up the Overlay pipeline
[x] Allow toggling overlay
```

### Phase III - Zone Management
*9/30/26 - present*

```
[x] Create a Zone struct
[x] Render Zone boundaries
[x] Assign pixels to zones in compute shader
[x] Read zones in map generation pipeline and set some base normal.
[x] Allow creating zones with bezier paths
[x] Modify zone paths, at path and vertex level, including deletion and insertion
[x] Joining zone paths at certain vertices, which should update both paths
    when manipulated
[x] Joining zone paths across multiple shapes
[ ] Selecting and manipulating multiple zones/vertices at once
    Shift+select in zone mode should select all connected zones.
    Shift+select in point mode should select all points in the clicked zone.
    Ctrl+select in point mode should select all points in all connected zones.
[ ] Selection options panel which allows you to set base normal for a zone,
    which will be applied before all shapes. Shapes will not influence past this
    base layer
[ ] Changing vertex mode in tool panel, and changing coupling with siblings.
    Changing out of linear mode should retain handle states if set, and if either
    is at 0, should infer a new handle. Either flipping other, or pointing into path.
    Even though that would change the path.
[ ] Joining selected vertices as siblings in tool panel (moves to avg. pos).
[x] Window preview mode for working normal map
```

### Phase IV - Editor Features

```
[ ] Undo/redo with history. Use a stack like Unity's Undo class. Look for an
    external crate first.
[ ] Copy and paste selected zones.
[ ] Rotate/scale selection
[ ] Holding shift/alt/etc while rotate/scale to snap/keep ratio/etc. All the things.
[ ] Saving workspace files (with path to sprite and normal map, all shapes,
    zones, and lights, but not camera)
[ ] Loading workspace files
[ ] Exporting normal map (Ctrl+E exports to path with confirmation if it
    would overwrite. Ctrl+Shift+E opens dialog.)
[ ] Having multiple workspace files open
[ ] Copy/pasting zones between workspace files
```

### Phase V - The Whole Point

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

### Phase VI - Extra Features

```
[ ] Slicing the grid into individual sprites, and displaying the grid
[ ] Copy/paste/move with sprite-level intervals
[ ] Timeline panel where sprites can easily be added
[ ] Display looping lit animation
[ ] Layer management, including temporarily disabling them.
[ ] Putting zones (and their control points) on different layers. Pixels will be assigned to the highest layer zone.
    This allows more complicated sprites where one object might pass in front of another.
[ ] Feathering between connected edges of zones, just between two control points. Difficult
```

### Interlude

```
[ ] Vital bug-fixes/polish
[ ] Use during development of Throwing Hands, until that project is finally
    done and released and I can move on with my life.
```

### Phase VII - Release

Finish, polish, and deploy as a commercial product.
Will add steps to this section as I think of them.
Will probably remove the GPL 3.0 license.

```
[ ] Better UI - final icons, repeatable welcome tutorial, etc.
[ ] Preferences (light/dark mode, transparency background, etc.)
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

## Keybinds Reference

This section will be added to as I add keybinds (and remember).

### Select tools

> `V` - Zone selection tool  
> `Shift + V` - Anchor point selection tool  
> `P` - Pen tool  

### Camera

> `WASD` - Pan camera. Hold shift to pan more quickly.  
> `Middle Click + Drag` - Pan camera.
> `Home` - Recenter camera.  
> `Shift + Scroll` - Zoom in or out.  
> `Ctrl + Plus` - Zoom in.  
> `Ctrl + Minus` - Zoom out.  

### Objects

> `Left Click + Drag` - Drag light or selected zones/control points/shapes.
> `PageUp/PageDown` - Adjust light height while dragging it.  
> `Escape` - Cancel zone path or shape creation.  
> `Delete` - Delete selected zone path/vertex or shape.


### Rendering

> `L` - Toggle lighting. If normal maps are on and light is off, the normal map itself is visible.  
> `Shift + L` - Toggle shading w/ normal map. If disabled with lighting on, shades flat.  
> `Ctrl + L` - Toggle both lighting and normal map. Allows quick swap between full shaded and flat unshaded sprites.

### Overlay

> `O` - Toggle entire overlay.  