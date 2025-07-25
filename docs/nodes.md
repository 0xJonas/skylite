# Nodes

Nodes are the basic building blocks of a Skylite project. They encapsulate sets of properties as well as the logic that handles updates and rendering. Most importantly, they can contain and manage other nodes, resulting in a tree structure that represents the entire project.

A node must first be declared by an asset file in Scheme, which provides any information that may be needed by other assets. For each node asset there must be a corresponding `node_definition` in Rust, which supplies the actual logic associated with the node.

## Asset File

A node asset is a Scheme file consisting of an associative list ("alist") with the following keys:

```scheme
'((parameters . (...))
  (properties . (...)))
```

The `parameters` key declares which parameters are needed to instantiate the node. This includes both instantiations in Rust via the `#[skylite_proc::new]` function, as well as instantiations in other Scheme assets such as [node lists](node_lists.md). The value for this key should be a list of [variable declarations](variables_and_types.md). When creating a node instance, in either Rust or Scheme, the arguments to the parameters must be given in the same order as the parameters are declared.

```scheme
; my-node.scm
'( ;...
  (parameters .
    ; Defines a parameter with name 'param1' of type 'u8'
    ((param1 u8)
    ; Defines a parameter with name 'param2' of type 'bool'
     (param2 bool)))
  ; ...
  )
```

`properties` declares which properties of a node are visible to other assets. Only fields on the node type that are referenced by asset files have to be declared here; if a field is only used in Rust, it need not be declared as a property. Like `parameters`, the value for this key should be a list of variable declarations.

```scheme
'( ;...
  (properties .
    ; Defines a property 'x-pos' of type 'i16'
    ((x-pos i16)
    ; Defines a property 'y-pos' of type 'i16'
     (y-pos i16)))
  ; ...
  )
```

Both keys are optional, so the simplest node asset is the empty list `'()`.

Parameters and properties are reflected in Rust inside the corresponding `#[node_definition]`.

## `#[node_definition(...)]`

For each node asset, there must be a corresponding `#[node_definition(...)]` in Rust. `#[node_definition(...)]` is an attribute macro that must be applied to a `mod`, which contains the code and type definitions associated with the node.

The attribute macro takes two parameters: The first parameter must be the path to the [main project file](project_files.md#main-project-file), the second parameter is the [asset name](project_files.md#asset-files) of the node asset associated with the node definition:

```Rust
#[node_definition("./project/my-project.scm", "my-node")]
mod def {
    struct MyNode {
        // Properties and child nodes
    }

    impl MyNode {
        // node methods, including some special methods
    }

    // ...
}
```

The content of the node definition module must include two things: A struct definition for the node type, and an `impl` block for this type. Within the node definition module, attributes can be used to mark various special items, such as child nodes, or update and rendering methods. These attributes must always be written as `#[skylite_proc::<attribute>]`, even if `skylite_proc` is imported into the module.

The attribute macro will generate an implementation of the `skylite_core::nodes::Node` trait for the node type, given items inside the definition module.

Unless otherwise stated, special items are _optional_. Using the same attribute on multiple item results in a compile-time error.

The following sections cover which attributes can be used to which effect inside the node definition.

### Node Struct

The node struct is the type that represents a node in Rust code. The name of the node struct must be the name of the node asset converted to _UpperCamelCase_. The struct usually contains named fields, but it can also be a tuple struct or a unit struct. Note that only structs with named fields can contain child nodes and animated properties.

Within the node struct definition, there are a few attributes that can be used to mark special fields:

- `#[skylite_proc::property]`: Marks a property defined in the node asset. This field must use the same name as the property in the node asset converted to _lower_snake_case_, have a matching type, and must not be private.
- `#[skylite_proc::node]`: Marks a child node. A fields marked with this attribute must have a node as type.
- `#[skylite_proc::nodes]`: Marks a list of child nodes. The type of this field must have one of the following types:
  - A `Vec` or slice of a node type.
  - A `Vec` or slice of `Box<dyn Node<P=MyProject>>`

For a node to be considered a child node, and therefore be traversed during the update and render cycles (see the following sections), it must be marked with either `#[skylite_proc::node]` or `#[skylite_proc::nodes]`. `#[skylite_proc::node]` and `#[skylite_proc::property]` can both be used on the same field to enable animating properties of child nodes. Using both `#[skylite_proc::node]` and `#[skylite_proc::nodes]` on the same field is not allowed. Using either of these attributes on a field that does not have a matching type will cause compile-time errors in generated code, so if you receive strange looking errors messages related to node iteration functions, this may be the cause. If a field is not marked with any attribute, it does not receive any special properties.

### Node Instantiation

A node must provide a way to instantiate it. This is done by providing an associated function marked with `#[skylite_proc::new]`, which receives the parameters declared in the node asset and returns a new instance of the node. For the example asset described earlier, this function should have the following signature:

```rust
#[skylite_proc::new]
fn new(param1: u8, param2: bool) -> MyNode {
    // ...
}
```

The actual name of the function, like most special functions or methods, is not important. The function only needs to be marked with `#[skylite_proc::new]` and have an appropriate signature.

This function is always **required**.

### Update Cycle Special Methods

The update cycle is performed each time the `update` method on a `SkyliteProject` is called. This is the time when a node should make changes to its fields.

During the update cycle, the node tree is traversed and a node's update functions are called. There are two functions which handle updates to a node:

#### `#[skylite_proc::update]`

This function is called after all of a node's children have been updated. Because of this, there is an alias for this attribute called `#[skylite_proc::post_update]`. A function marked with either attribute should take a mutable reference to the node type and a mutable reference to the project's control type:

```rust
#[skylite_proc::update]
fn update(&mut self, controls: &mut ProjectControls<MyProject>) {
    // ...
}
```

#### `skylite_proc::pre_update`

This function is called before any of the node's children are updated. It takes the same arguments as the `#[skylite_proc::post_update]` function:

```rust
#[skylite_proc::pre_update]
fn pre_update(&mut self, controls: &mut ProjectControls<MyProject>) {
    // ...
}
```

### Render Cycle Special Methods

The render cycle is responsible for drawing each node to the screen. A render cycle is performed whenever the `render` method on a `SkyliteProject` is called. Nodes can _not_ be modified during a render cycle, in fact, user code should make no assumptions on how many render cycles are performed between successive update cycles.

Nodes are rendered in a different order than how they are updated, to make it easier to control which nodes are drawn on top of other nodes. The following methods on a node control rendering:

#### `#[skylite_proc::render]`

This function does the main drawing to the screen. It receives a shared reference to the node and a `DrawContext` which holds various information needed for rendering:

```rust
#[skylite_proc::render]
fn render(&self, ctx: &mut DrawContext<MyProject>) {
    // ...
}
```

#### `#[skylite_proc::z_order]`

Controls when this node is drawn in relation to other nodes. Nodes with a higher z-order are drawn on top of nodes with lower z-order. Nodes with the same z-order are drawn in a stable order. If no z-order function is specified, the default z-order is 1. A function marked with the `#[skylite_proc::z_order]` attribute should have the following signature:

```rust
#[skylite_proc::z_order]
fn z_order(&self) -> i32 {
    // ...
}
```

#### `#[skylite_proc::is_visible]`

Controls whether this node should be drawn at all. This function takes a shared reference to the node instance and a shared reference to a `DrawContext`. Only if this function returns true, the node is rendered, otherwise it is skipped during rendering. If this function is not implemented, the default behavior depends on whether a function with attribute `#[skylite_proc::render]` is defined: If there is a render function, the default `#[skylite_proc::is_visible]` function always returns true, otherwise it returns false.

```rust
#[skylite_proc::is_visible]
fn is_visible(&self, ctx: &DrawContext<MyProject>) -> bool {
    // ...
}
```
