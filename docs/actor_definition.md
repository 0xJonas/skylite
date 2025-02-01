# The `actor_definition!` Macro

An `actor_definition` is the Rust counterpart to an [actor asset file](actor_assets.md) and provides implementations for the actor's actions and periodic method calls. The body of an `actor_definition` contains function items, which may be marked with specific attributes, as well as function macro calls to set additional information about the actor.

The `actor_definition` macro defines all types needed for an actor to be used in a scene. This includes the following types:

- The actor's **main type**, which has the same name as the asset file converted to *UpperCamelCase*. E.g. an asset file with name `my_actor` would have a **main type** called `MyActor`. This is also the type that implements the `Actor` trait.
- The actor's **action type**, which is an `enum` with a variant for each action defined in the asset file. Each variant is a struct containing the parameters for the respective action. The action type's name is the name of the **main type** with `Actions` appended to it, the names of the variants are the names of the actions converted to *UpperCamelCase*. E.g. an asset file with name `my_actor` would have a **action type** called `MyActorActions`.
- The actors **property type**: This type is always generated, even when the actor does not define any properties. The name of this type is the name of the **main type** with `Properties` appended to it. E.g. an asset file with name `my_actor` would have a **property type** called `MyActorProperties`. Each instance of the actor's main type contains an instance of the actor's property type, which is accessible through the `.properties` member on the main type.

Calls to `actor_definition` must be made visible (e.g. through `use`) to the projects main `mod` marked with the `#[skylite_project(...)]` attribute.

## Special Functions and Macros

Within the body of `actor_definition`, there are several items with special meanings. These can be either calls to function macros, or attribute macros added to regular functions. These macros and attributes must always be written using an absolute path starting with `skylite_proc::`, regardless of any `use`-lines that might be present.

- `skylite_proc::asset_file!(...);`

  Sets the asset file for this `actor_definition`. The first argument is the path to the project's main definition file relative to the project's root directory, the second argument is the name of the [actor asset file](actor_assets.md) without the file extension:

  ```rust
  skylite_proc::asset_file!("path/project.scm", "asset-name");
  ```

  This macro invocation is always **required**.

- `skylite_proc::properties! { ... }`

  Declares the properties of the actor, which are accessible through the actor's **property type**. Properties are the data that an actor instance holds and that can be changed by user code, e.g. through action implementations. The properties are declared in the same way as the members of a `struct`:

  ```rust
  skylite_proc::properties! {
      pub x: u16,
      pub y: u16
  }
  ```

  In order for the properties to be visible to the other items in the `actor_definition`, they must be declared as `pub`.

  Properties are separate from the parameters, which are defined in the actor asset file. The properties are initialized through the `#[skylite_proc::create_properties]` special function (see below).

- `#[skylite_proc::create_properties]`

  Marks the function that initializes the properties of an actor, based on the arguments to the actors parameters. The properties of an actor are declared using the `skylite_proc::properties` macro, the parameters are declared in the actor's asset file.

  The function marked by this attribute must have a signature that matches the order and type of the actor's parameters, and must return an instance of the actor's **property type**.

  This function is **required** if `skylite_proc::properties` is used.

  Example:
  ```rust
  #[skylite_proc::create_properties]
  fn create_properties(x: i16, y: i16) -> MyActorProperties {
      MyActorProperties { x, y }
  }
  ```

- `#[skylite_proc::pre_update]`

  Marks a function that is called at the beginning of an update. The function marked by this attribute must take exactly the following parameters:
  - A mutable reference to the actor's **main type**.
  - A mutable reference to a `Scene` trait object (`&mut dyn Scene<P=MyProject>`)
  - A mutable reference to the `ProjectControls` instance (`&mut ProjectControls<MyProject>`)

  Example:
  ```rust
  #[skylite_proc::post_update]
  fn pre_update(actor: &mut MyActor, project: &mut MyProject) { /**/ }
  ```

- `#[skylite_proc::post_update]`

  Similar to `#[skylite_proc::pre_update]`, except that the marked function is called at the end of an update, instead of at the beginning.

- `#[skylite_proc::render]`

  Marks a function that is called to draw the actor to the screen. The function marked by this attribute must take exactly the following parameters:
  - An immutable reference to the actor's **main type**.
  - An immutable reference to a `DrawContext`.

  Example:
  ```rust
  #[skylite_proc::render]
  fn render(actor: &MyActor, ctx: &DrawContext<MyProject>) { /**/ }
  ```

- `#[skylite_proc::action("name")]`

  Marks an action implementation. The implementation of the actor's current action is the main function that is being run when the actor is updated. Each action declared in the asset file must have a matching implementation function inside `actor_definition`.

  The `"name"` argument to the attribute must be the exact name of an action from the asset file, without any case changes applied to it (so it can include `'-'` characters, which would be illegal in Rust).

  The functions marked by this attribute must take the following parameters:
  - A mutable reference to the actor's **main type**.
  - A mutable reference to a `Scene` trait object (`&mut dyn Scene<P=MyProject>`)
  - A mutable reference to the `ProjectControls` instance (`&mut ProjectControls<MyProject>`)
  - Any parameters to the action that were defined in the asset file, in the same order and with the same types.

  Example:
  ```rust
  #[skylite_proc::action("move")]
  fn move(actor: &mut MyActor, project. &mut MyProject, dx: i8, dy: i8) { /**/ }
  ```

## Complete Example

The following is a possible implementation for the asset file from [this example](actor_assets.md#complete-example):

```rust
// This macro invocation will generate the types `MyActor`, `MyActorActions` and `MyActorProperties`.
skylite_proc::actor_definition! {

    // Sets the asset file to be a file with name "my_actor", which is part of the
    // actor assets of the project at "path/project.scm".
    // The name "my_actor" is converted to `MyActor` for the names of the types
    // generated by `actor_definition!`.
    skylite_proc::asset_file!("path/project.scm", "my_actor");

    // These properties will form the body of the type `MyActorProperties`
    skylite_proc::properties {
        pub x: i16,
        pub y: i16
    }

    // Mark the function which is used to initialize the actor's properties from its parameters.
    #[skylite_proc::create_properties]
    fn create_properties(x: i16, y: i16) -> MyActorProperties {
        MyActorProperties { x, y }
    }

    // Provide implementations to the actor's actions.

    #[skylite_proc::action("move")]
    fn r#move(actor: &mut MyActor, _scene: &mut Scene<P=MyProject>, _controls: &mut ProjectControls<MyProject>, dx: i8, dy: i8) {
        actor.properties.x += dx as i16;
        actor.properties.y += dy as i16;
    }

    #[skylite_proc::action("idle")]
    fn idle(_actor: &mut MyActor, _scene: &mut Scene<P=MyProject>, _controls: &mut ProjectControls<MyProject>) {}

    #[skylite_proc::action("set-position")]
    fn set_position(actor: &mut MyActor, _scene: &mut Scene<P=MyProject>, _controls: &mut ProjectControls<MyProject>, x: i16, y: i16) {
        actor.properties.x = x;
        actor.properties.y = y;

        // Change the current action by using a variant from the actor's action type.
        actor.set_action(MyActorActions::Idle {});
    }

    #[skylite_proc::render]
    fn render(actor: &MyActor, ctx: &DrawContext<MyProject>) {
        // Draw something to the screen.
    }
}
```
