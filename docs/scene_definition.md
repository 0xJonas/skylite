# The `scene_definition!` Macro

The `scene_definition!` macro is the counterpart to a [scene asset file](scene_assets.md). Each scene asset must have a corresponding `scene_definition!` call inside your Rust code, which provides the types, trait implementations and custom code for the scene. The required code and definitions are given in the body of the `scene_definition!` call as special macro invocations and function items.

`scene_definition!` generates the following types:
- **Main scene type**: This is the main type that represents the scene. It contains the data associated with the scene instance, such as actor lists and custom properties. This type also implements the `Scene` trait. This type has the same name as the scene asset converted to *UpperCamelCase*.
- **Properties type**: Contains custom properties defined by the `skylite_proc::properties!` macro (see below). Each scene contains an instance of this type, which is accessible through the `.properties` member of the main scene type. The name of this type is the name of the main type with `Properties` appended to it.
- `**Named actors type**`: This is an enum that lists the names of the named actors in order of definition. The enum has a `usize` representation, which allows it to be used to index into the scene's list of named actors, e.g. `scene.get_actors()[MySceneActors::Actor1]`. The name of the enum is the name of the main type with `Actors` appended to it, the name of each enum variant is name of the actor instance, converted to *UpperCamelCase*.

## Special Functions and Macros

Within a `scene_definition!` there are multiple macros and attributes that have special meanings, like defining properties or marking function to be called at a particular time. In order to be recognized, all of these macros must be given with an absolute path starting with `skylite_proc::`.

- `skylite_proc::asset_file!(...);`

  Links this `scene_definition!` to a scene asset. This macro takes two string literals as arguments, the first is the path to the project's root file and the second is the name of a scene asset, without the file extension.

  ```rust
  skylite_proc::asset_file!("path/project.scm", "my_scene");
  ```

  This macro is always **required**.

- `skylite_proc::properties! { ... }`

  Defines custom properties for the scene type, which are used as the content for the **properties type**. The content of this macro should be a list of member declarations, like the content of a Rust `struct`:

  ```rust
  skylite_proc::properties! {
      pub val1: bool,
      pub val2: u8
  }
  ```

  Properties are separate from the parameters, which are defined in the actor asset file. The properties are initialized through the `#[skylite_proc::create_properties]` special function (see below).

  This macro is not required, but if it is missing, an empty property type is still generated.

- `#[skylite_proc::create_properties]`

  Marks a function that creates an instance of the scene's **property type**. The marked function receives arguments to the parameters defined in the scene asset and must return an instance of the scene's property type:

  ```rust
  #[skylite_proc::create_properties]
  fn create_properties(param1: bool, param2: u8) {
      MySceneProperties {
          val1: param1, val2: param2
      }
  }
  ```

  This function is **required if** there are parameters defined in the scene asset, or if there is a call to `skylite_proc::properties` within the `scene_definition`.

- `#[skylite_proc::init]`

  Marks a function used to initialize the scene instance. The marked function should take a mutable reference to the newly create instance of the main type, as well as arguments to the parameters defined in the scene asset:

  ```rust
  #[skylite_proc::init]
  fn init(scene: &mut MyScene, param1: bool, param2: u8) { ... }
  ```

  This function can be used to make initial modifications to the actors or extras.

- `#[skylite_proc::pre_update]`

  Marks a function that is called at the beginning of an update. The marked function should take the following parameters:
  - A mutable reference to of the scene's **main type**: `&mut MyScene`.
  - A mutable reference to the project's control type: `&mut ProjectControls<MyProject>`.

- `#[skylite_proc::post_update]`

  Like `#[skylite_proc::pre_update]`, but the marked function is instead called at the end of an update.

- `#[skylite_proc::pre_render]`

  Marks a function that is called at the beginning of rendering the scene. The marked function should take the following parameters.
  - An immutable reference to the scene's **main type**: `&MyScene`.
  - An immutable reference to a `DrawContext`: `&DrawContext<MyProject>`.

- `#[skylite_proc::post_render]`

  Like `#[skylite_proc::pre_render]`, but the marked function is instead called at the end of rendering the scene.

## Complete Example

Here is an example `scene_definition!` for the scene asset from [Scene Asset File Format](scene_assets.md):

```rust
scene_definition! {
    use crate::my_project::*;

    skylite_proc::asset_file!("path/project.scm", "my_scene");

    skylite_proc::properties! {
        pub val1: bool,
        pub val2: u8
    }

    #[skylite_proc::create_properties]
    fn create_properties(param1: bool, param2: u8) -> MySceneProperties {
        MySceneProperties {
            val1: param1, val2: param2
        }
    }

    #[skylite_proc::init]
    fn init(scene: &mut MyScene, param1: bool, param2: u8) {
        // ...
    }

    #[skylite_proc::pre_update]
    fn pre_update(scene: &mut MyScene, controls: &mut ProjectControls<MyProject>) {
        // ...
    }

    #[skylite_proc::post_render]
    fn post_render(scene: &MyScene, ctx: &DrawContext<MyProject>) {
        // ...
    }
}
```
