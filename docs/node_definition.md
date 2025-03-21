# The `node_definition` Macro

For each [node asset](node_assets.md), there must be a corresponding `node_definition!` somewhere in the project's Rust code. The `node_definition` provides the code that is used for various operations on Nodes, such as initialization, updating and rendering.

A `node_definition` consists of the outer macro call, and a body which contains any number of items. These items can include several **special items**, which are marked with a particular attribute which denotes their specific function for the Node.

```rust
node_definition! {
    skylite_proc::asset_file!("project/project.scm", "my-node");

    #[skylite_proc::update]
    fn update(node: &mut MyNode, controls: &mut ProjectControls<MyProject>) {
        // ...
    }
}
```

The attributes and macros inside a `node_definition` must be given with their full path name, so the `skylite_proc::`-prefix is always required. Unless otherwise stated, special items are *optional*. Special items are only recognized by their attribute, the actual name of the item is not important. Special function items are required to take certain parameters, based on the attribute used, which are described in the sections for the respective attribute. The body of a `node_definition` is a separate scope, so multiple `node_definitions` in the same module can use the same identifiers for their items.

## Node Type

A `node_definition` generates the main **node type**, as well as types for its fields:
```rust
// For node asset "my-node"
struct MyNode {
    properties: MyNodeProperties,
    static_nodes: MyNodeStaticNodes,
    dynamic_nodes: Vec<Box<dyn Node<P=MyProject>>>
}
```

The name of the node type is the name of the node asset converted to UpperCamelCase.

The `properties` field holds the instance of the Node's **properties type**. The properties type is a `struct` which contains the properties defined in the [node asset](node_assets.md#properties), as well as those given in the [`skylite_proc::extra_properties!` macro](#initialization). The properties from the node asset are converted into struct fields, where the name of the field is the name of the property converted to lower_snake_case, and the type is the Rust equivalent according to [Type Conversion](variables_and_types.md#type-conversion). The name of the properties type is the name of the node type followed by `Properties`.

The `static_nodes` fields contains the the static nodes defined in the [node asset](node_assets.md#static-nodes). The type of this field is the Node's **static nodes type**. Each static node is converted into a field on this type, where the name of the field is the name of the static node converted to `lower_snake_case` and the type is the **node type** of the respective node. The name of the static node type is that of the main node type with `StaticNodes` appended to it.

`dynamic_nodes` contains this Node's dynamic child nodes. See [Node Assets](node_assets.md) for an explanation of the difference between static and dynamic child nodes.

If a Node is only available as a trait object, child nodes can also be accessed through the `get_static_nodes` and `get_dynamic_nodes` methods, as well as their `*_mut` counterparts, on the `Node` trait.

### `skylite_proc::asset_file!`

This macro associates a `node_definition` with a node asset. `skylite_proc::asset_file` is a function macro that takes two arguments: The first argument is the path to the project's main definition file, the second is the name of the node asset. E.g. if there is a file `my-node.scm` in the node asset directory, the `skylite_proc::asset_file!` invocation would look like this:

```rust
skylite_proc::asset_file!("path/to/project.scm", "my-node");
```

This macro is always **required**.

### `skylite_proc::extra_properties!`

Allows defining additional properties that are not given in the node asset. This can be used to define properties that are not restricted to the types available for node assets (see [Variables and Types](variables_and_types.md)). However, properties defined through the `skylite_proc::extra_properties!` macro cannot be animated by sequences.

The body of the `skylite_proc::extra_properties!` macro should be a list of field definitions, similar to a struct. Fields must be at least `pub(super)` to be accessible by items in the `node_definition`.

```rust
skylite_proc::extra_properties! {
    pub extra_box: Box<u32>,
    pub extra_thing: CustomThing
}
```

## Initialization

When a new instance of a Node is created, it must first be initialized. There are multiple steps to initialization:

### `#[skylite_proc::create_properties]`

A function marked with this attribute is responsible for creating the instance of the Node's property type. The marked function receives arguments to the parameters defined in the [node asset](node_assets.md#parameters), in the order they are given there, and should return a new instance of the property type. Unless the node defines no properties or `skylite_proc::extra_properties`, this function is **required**.

```rust
#[skylite_proc::create_properties]
fn create_properties(param1: u8, param2: u8) -> MyNodeProperties {
    MyNodeProperties { /*...*/ }
}
```

### `#[skylite_proc::init]`

This function can be used to run custom setup code for a newly created Node. It is called when all fields of the node type are fully initialized. It receives a mutable reference to the new Node instance, as well as arguments to the Node's parameters.

```rust
#[skylite_proc::init]
fn init(node: &mut MyNode, param1: u8, param2: u8) {
    // ...
}
```

## Update Cycle

The update cycle is performed each time the `update` method on a `SkyliteProject` is called. This is the time when a node should change its internal properties.

During the update cycle, the node tree is traversed and a node's update functions are called. There are two functions which handle updates to a node:

### `#[skylite_proc::update]`

This function is called after all of a node's children have been updated. Because of this, there is an alias for this attribute called `#[skylite_proc::post_update]`. A function marked with either attribute should take a mutable reference to the Node type and a mutable reference to the project's control type:

  ```rust
  #[skylite_proc::update]
  fn update(node: &mut MyNode, controls: &mut ProjectControls<MyProject>) {
      // ...
  }
  ```

### `skylite_proc::pre_update`

This function is called before any of the node's children are updated. It takes the same arguments as the `#[skylite_proc::post_update]` function:

  ```rust
  #[skylite_proc::pre_update]
  fn pre_update(node: &mut MyNode, controls: &mut ProjectControls<MyProject>) {
      // ...
  }
  ```

## Render Cycle

The render cycle is responsible for drawing each node to the screen. A render cycle is performed when the `render` method on a `SkyliteProject` is called. Node's can *not* be modified during a render cycle, in fact, user code should make no assumptions on how many render cycles are performed between successive update cycles.

Nodes are rendered in a different order than how they are updated, to make it easier to control which nodes are drawn on top of other nodes. The following methods on a node control rendering:

### `#[skylite_proc::render]`

This function does the main drawing to the screen. It receives a shared reference to the Node and a `DrawContext` which holds various information needed for rendering:

```rust
#[skylite_proc::render]
fn render(node: &MyNode, ctx: &mut DrawContext<MyProject>) {
    // ...
}
```

### `#[skylite_proc::z_order]`

Controls when this node is drawn in relation to other nodes. Nodes with a higher z-order are drawn on top of nodes with lower z-order. Nodes with the same z-order are drawn in a stable order. If no z-order function is specified, the default z-order is 1. A function marked with the `#[skylite_proc::z_order]` attribute should take a shared reference to the Node instance and return an `i32` as the Z-order.

```rust
#[skylite_proc::z_order]
fn z_order(node: &MyNode) -> i32 {
    // ...
}
```

### `#[skylite_proc::is_visible]`

Controls whether this node should be drawn at all. This function takes a shared reference to the Node instance and a shared reference to a `DrawContext`. Only if this function returns true, the Node is rendered, otherwise it is skipped during rendering. If this function is not implemented, the default behavior depends on whether a function with attribute `#[skylite_proc::render]` is defined: If there is a render function, the default `#[skylite_proc::is_visible]` function always returns true, otherwise it returns false.

```rust
#[skylite_proc::is_visible]
fn is_visible(node: &MyNode, ctx: &DrawContext<MyProject>) -> bool {
    // ...
}
