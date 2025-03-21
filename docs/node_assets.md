# Node Assets

Nodes are the primary building blocks of Skylite projects. Every entity in a project consists of one or more nodes, where each node fulfils a specific role, such as providing a position on the screen, handling special behavior, or managing a collection of other nodes. All nodes in a project form a tree structure, where each node can have two sets of child nodes, **static nodes** and **dynamic nodes**:
- The set of static nodes is fixed at build time by the node asset and cannot be modified at runtime. These nodes can be accessed by the parent node through `<parent>.static_nodes.<child>`. The type of the child type is known at compile time, allowing easy access to the child node's properties. The properties of static child nodes can be animated by sequences on the parent.
- The list of dynamic nodes is initialized by the node asset, but nodes can be added or removed at runtime. Dynamic nodes are accessible to the parent through through the `<parent>.dynamic_nodes` property, which is a `Vec<Box<dyn Node<P=MyProject>>>`. Properties of dynamic child nodes cannot be animated by sequences on the parent.

## Asset Format

Node assets are defined by a scheme file, where the name of the file without the extension is the name of the node. Node names are used both throughout other asset files and generated code, potentially with changed casing (see [Identifiers](variables_and_types.md#identifiers)).

A Node assets contains an associative list ("alist") at the top level:

```scheme
'((parameters . ())
  (properties . ())
  (static-nodes . ())
  (dynamic-nodes . ()))
```

All of these keys are optional; if a key is omitted it is simply treated as if an empty list has been given.

### `parameters`

This key defines the parameters that are used to construct and initialize a instance of the node. The value of the `parameters` key should be a list of [variable definitions](variables_and_types.md), consisting of name, type and an optional description:

```scheme
; my-node.scm
'( ;...
  (parameters .
    ; Defines a parameter with name 'param1' of type 'u8' with description "First parameter"
    ((param1 u8 "First parameter")
    ; Defines a parameter with name 'param2' of type 'bool
     (param2 bool)))
  ; ...
  )
```

Values for these parameters must be supplied when this node is instantiated in other assets, as well as in code:

```scheme
(my-node 5 #t)
```

```rust
MyNode::new(5, true)
```

### `properties`

The `properties` key defines the data that is actually stored within the node. Properties can be initialized by the [`create_properties`](node_definition.md#skylite_proccreate_properties) function, which also receives arguments to the Node's parameters. Properties can be accessed through the `properties` member on the generated Node type. Additional properties can be defined by the [`extra_properties!`](node_definition.md#skylite_procextra_properties) macro in the `node_definition!`, but only the properties from the asset file can be animated by sequences. The value for the `properties`-key is a list of variable definitions, similar to `parameters`:

```scheme
'( ;...
  (properties .
    ; Defines a property 'x-pos' of type 'i16', with description "X position".
    ((x-pos i16 "X position")
    ; Defines a property 'y-pos' of type 'i16', with description "Y position".
     (y-pos i16 "Y position")))
  ; ...
  )
```

### `static-nodes`

This key defines the set of static nodes. These nodes are available through the `static_nodes` field on the [Node type](node_definition.md#node-type). The value for this key is an alist from identifiers to node instances. Nodes are instantiated in asset files by creating a list with the name of the node as the first element and the arguments to the node as the following elements:

```scheme
; my-node.scm
'( ;...
  (static-nodes .
    ; Define static node 'node1' as an instance of 'my-other-node' with arguments 10, 10.
    ((node1 . (my-other-node 10 10))
    ; Define static node 'node2' as an instance of 'my-other-node' with arguments 20, 20.
     (node2 . (my-other-node 20 20))))
  ;...
  )
```

### `dynamic-nodes`

Defines the initial list of dynamic nodes. This list can be modified through the `dynamic_nodes` member on the [Node type](node_definition.md#node-type). The value for `dynamic-nodes` should be a list of Node instances:

```scheme
'( ;...
  (dynamic-nodes .
    ; Adds two instance of 'my-third-node' one with argument #t, and one with #f.
    ((my-third-node #t)
     (my-third-node #f)))
  ; ...
  )
```

## Example

Here is a complete example of a node asset file:

```scheme
; my-node.scm
'((parameters .
    ((param1 u8 "First parameter")
     (param2 bool)))
  (properties .
    ((x-pos i16 "X position")
     (y-pos i16 "Y position")))
  (static-nodes .
    ((node1 . (my-other-node 10 10))
     (node2 . (my-other-node 20 20))))
  (dynamic-nodes .
    ((my-third-node #t)
     (my-third-node #f))))
```
