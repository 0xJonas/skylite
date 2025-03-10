use scenes::SceneParams;

pub mod actors;
pub mod decode;
pub mod ecs;
pub mod nodes;
pub mod scenes;

/// Defines which functions a backend must provide to work with Skylite.
pub trait SkyliteTarget {
    /// Draws a region from a texture atlas to the screen.
    ///
    /// The texture atlas is given as the `data` parameter. There is no fixed
    /// format for the data, so it is up to the implementation to interpret
    /// the remaining parameters apropriately. The data will always be the
    /// complete content of a single graphics file.
    ///
    /// The position where the region should be drawn on the screen is given by
    /// the `x` and `y` parameters, with (0, 0) being the top-left corner.
    ///
    /// The region of the atlas is defined by `src_x` and `src_y` for the
    /// position, and `src_w` and `src_h` for the width and height.
    ///
    /// If `flip_h` is true, the region should be mirrored horizontally before
    /// drawing. If `flip_v` is true, the region should be flipped
    /// vertically. If `rotate` is true, the region should be rotated 90 degrees
    /// clockwise. Rotation is applied after flipping.
    fn draw_sub(
        &mut self,
        data: &[u8],
        x: i16,
        y: i16,
        src_x: i16,
        src_y: i16,
        src_w: u16,
        src_h: u16,
        flip_h: bool,
        flip_v: bool,
        rotate: bool,
    );

    /// Returns the screen size of the target as a (width, height) tuple.
    /// This must always return the same value during the lifetime of the
    /// instance.
    fn get_screen_size(&self) -> (u16, u16);

    /// Writes the given data at the given offset into persistent storage.
    fn write_storage(&mut self, offset: usize, data: &[u8]);

    /// Reads some amount of data from persistent storage, starting at the given
    /// offset.
    fn read_storage(&self, offset: usize, len: usize) -> Vec<u8>;
}

/// The main type for skylite projects.
pub trait SkyliteProject {
    type Target: SkyliteTarget;
    type TileType: Copy;
    type SceneParams: SceneParams<P = Self>;

    /// Creates a new instance of the project with the given target.
    fn new(target: Self::Target) -> Self;

    /// Performs a single render cycle.
    ///
    /// Rendering, if implemented correctly in user code, should not
    /// change any state in the scenes or actors, so if `render` is called
    /// multiple times without intermediate updates, the output should stay the
    /// same.
    fn render(&mut self);

    /// Performs a single update cycle.
    ///
    /// This operation changes the state of scenes and actors.
    fn update(&mut self);

    /// Loads a new scene and sets the current scene of the project to it.
    fn set_scene(&mut self, params: Self::SceneParams);
}

/// Holds the rendering state.
///
/// The `DrawContext` contains all information required for graphics
/// rendering, such as a handle of the current [`SkyliteTarget`],
/// the cache for the currently loaded graphics, or the current camera focus.
pub struct DrawContext<'project, P: SkyliteProject> {
    #[doc(hidden)]
    pub target: &'project mut P::Target,
    #[doc(hidden)]
    pub graphics_cache: &'project mut Vec<std::rc::Weak<u8>>,
    #[doc(hidden)]
    pub focus_x: i32,
    #[doc(hidden)]
    pub focus_y: i32,
}

/// Type used to change various parts of a `SkyliteProject` instance.
///
/// This is the main type that scenes and actors have access to in their
/// update/action methods.
pub struct ProjectControls<'project, P: SkyliteProject> {
    pub target: &'project mut P::Target,
    #[doc(hidden)]
    pub pending_scene: Option<P::SceneParams>,
}
