# Node Lists

When a node contains a long list of child nodes, such as for a list of actors within a level, these nodes can be stored in a dedicated *Node list*. Node lists are an asset that consists simply of a sequence of nodes that can be loaded and passed around. Internally, node lists are stored in a compressed format.

## Scheme

Node list assets are defined as a flat Scheme list of node instances (see [here](variables_and_types.md#asset-types) for the syntax of node instances):

```scheme
; node-lists/my-list.scm
'((my-node 1) (my-node 2) (my-node 3))
```

The name of the node list is the name of the asset file without its file extension.

## Rust

Accessing a node list in Rust is done through the node list's id. For each node list asset, `skylite_project` generates an id, which are collected in a dedicated enum type available on the main project type:

```rust
MyProject::NodeListIds
```

This id can then be used to retrieve the actual contents of the node list in the form of a `NodeList` instance:

```rust
let list1 = NodeList::load(MyProject::NodeListIds::MyList);
```
