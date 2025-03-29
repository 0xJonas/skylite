# Node Lists

When a node contains a large number of dynamic child nodes, such as for a list of actors within a level, these nodes can be stored in a dedicated *Node List*. Node Lists are an asset that consists simply of a sequence of nodes that can be loaded and passed around. Internally, node lists are stored in a compressed format.

## Scheme

Node List assets are defined as a flat Scheme list of node instances (see [here](node_assets.md#static-nodes) for the syntax of node instances):

```scheme
; node-lists/my-list.scm
'((my-node 1) (my-node 2) (my-node 3))
```

The name of the Node List is the name of the asset file without its file extension.

## Rust

There are three stages to a Node List in Rust:
1. Node List id: For each Node List asset, `skylite_project` generates an id. These ids are collected in a dedicated enum type available on the main project type:

   ```rust
   MyProject::NodeListIds
   ```

2. `NodeList`: The main Rust type representing the Node List. Instance of `NodeList` can be created by either loading one of the assets by passing the Node List id to `NodeList::load()`, or by passing an ad-hoc `Vec` or Nodes to `NodeList::new()`.

   ```rust
   let list1 = NodeList::load(MyProject::NodeListIds::MyList);

   let list2 = NodeList::new(vec![
       Box::new(MyNode::new(1)),
       Box::new(MyNode::new(2)),
   ]);
   ```
3. `SList`: A built-in Node that takes a `NodeList` as a parameter and adds the content to its [dynamic Nodes](node_assets.md). This node can be used to easily add the contents of a Node List to the Node tree. Within Scheme, this Node has the name `s-list` and takes a node list as a parameter (see also [asset types](variables_and_types.md#asset-types)):

   ```scheme
   '(s-list my-list)
   ```
