use actors::AnyActor;
use scenes::Scene;

pub mod decode;
pub mod scenes;
pub mod actors;

/// Defines which functions a backend must provide to work with Skylite.
pub trait SkyliteTarget {

    /// Draws a region from a texture atlas to the screen.
    ///
    /// The texture atlas is given as the `data` parameter. There is no fixed format for the data,
    /// so it is up to the implementation to interpret the remaining parameters apropriately. The data
    /// will always be the complete content of a single graphics file.
    ///
    /// The position where the region should be drawn on the screen is given by the `x` and `y` parameters,
    /// with (0, 0) being the top-left corner.
    ///
    /// The region of the atlas is defined by `src_x` and `src_y` for the position, and `src_w` and `src_h`
    /// for the width and height.
    ///
    /// If `flip_h` is true, the region should be mirrored horizontally before drawing. If `flip_v` is true,
    /// the region should be flipped vertically. If `rotate` is true, the region should be rotated 90 degrees
    /// clockwise. Rotation is applied after flipping.
    fn draw_sub(&mut self, data: &[u8], x: i16, y: i16, src_x: i16, src_y: i16, src_w: u16, src_h: u16, flip_h: bool, flip_v: bool, rotate: bool);

    /// Draws a region from a texture atlas to the screen as a tile.
    ///
    /// The specifics of how tiles are drawn depends on the target, but
    /// they should generally have a fixed size and be layed out in a regular grid.
    /// Tiles can be drawn on different layers, where each layer may have separate
    /// tile sizes, offsets and other configurations, depending on the capabilities
    /// of the target.
    ///
    /// `tile_x_idx` and `tile_y_idx` give the x and y index into the tile grid at which
    /// the tile should be drawn. `src_x` and `src_y` denote the top-left coordinate
    /// in the texture atlas in pixels. `flip_h`, `flip_y` and `rotate` work the same
    /// as in `draw_sub`.
    fn draw_tile(&mut self, data: &[u8], layer: u8, tile_x_idx: i16, tile_y_idx: i16, src_x: i16, src_y: i16, flip_h: bool, flip_v: bool, rotate: bool);

    /// Changes some configurable aspect of a layer. The parameters
    /// available and the corresponding values for `config` are
    /// specific to each target, but could include things like
    /// the tile dimensions or the current scroll offset.
    fn layer_cfg(&mut self, layer: u8, config: u32, data: &[u8]);

    /// Returns the screen size of the target as a (width, height) tuple.
    /// This must always return the same value during the lifetime of the instance.
    fn get_screen_size(&self) -> (u16, u16);

    /// Saves the given data at the specified location. `location` can be any arbitrary string.
    ///
    /// If the data exceeds the capacity of the location, this function should panic.
    fn save_state(&mut self, location: &str, data: &[u8]);

    /// Loads data from the given location. `location` can be any arbitrary string.
    fn load_state(&self, location: &str) -> Vec<u8>;
}

/// The main type for skylite projects.
pub trait SkyliteProject {
    type Target: SkyliteTarget;
    type TileType: Copy;
    type Actors: AnyActor<P = Self>;

    fn new(target: Self::Target) -> Self;
    fn render(&mut self);
    fn update(&mut self);
}

/// Holds the rendering state.
///
/// The `DrawContext` contains all information required for graphics
/// rendering, such as a handle of the current [`SkyliteTarget`],
/// the cache for the currently loaded graphics, or the current camera focus.
pub struct DrawContext<P: SkyliteProject> {
    #[doc(hidden)] pub target: P::Target,
    #[doc(hidden)] pub graphics_cache: Vec<std::rc::Weak<u8>>,
    #[doc(hidden)] pub focus_x: i32,
    #[doc(hidden)] pub focus_y: i32
}

/// Type used to change various parts of a `SkyliteProject` instance.
///
/// This is the main type that scenes and actors have access to in their
/// update/action methods.
pub struct ProjectControls<P: SkyliteProject> {
    #[doc(hidden)] pub pending_scene: Option<Box<dyn Scene<P=P>>>
}
