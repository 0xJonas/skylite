# Variables and Types

Throughout a project, there are some places where variables are defined in asset files and then used from Rust code, primarily as parameters and properties for [nodes](nodes.md#asset-file). All variables are made up of an identifier and a type, which both have different representations in different contexts.

Variables are declared using the following syntax:

```scheme
'(<name> <type>)
```

`<name>` must be a symbol giving the identifier of the variable. The syntax for `<type>` varies depending on the specific type and is described below.

## Identifiers

Variable identifiers initially defined in an asset file, like a node asset, generally need to be converted to a Rust variable. When an identifier is given a counterpart in Rust code, its casing is changed to match the variable's function in the code. It is important to note that the names of asset files are themselves identifiers which follow this same pattern.

An identifier can assume the following casings:

| Name             | Example     | Usage in Rust                                             |
| ---------------- | ----------- | --------------------------------------------------------- |
| Kebab-case       | `color-rgb` | In string literals, when the identifier is not normalized |
| Lower snake-case | `color_rgb` | Parameters, local variables.                              |
| Upper snake-case | `COLOR_RGB` | Global `const` and `static` items                         |
| Lower camel-case | `colorRgb`  | N/A                                                       |
| Upper camel-case | `ColorRgb`  | Type names and traits                                     |

When an identifier starts in Scheme, which has very liberal rules for its symbols, it can use a mix of different casings. This is not recommended however, since it can lead to surprises when the identifier is normalized in the generated Rust code.

## Types

A variable also has a type to specify what kind of data the variable holds and which values are allowed to be used for this variable in other assets. To convert between the asset files' Scheme format and Rust syntax, a fixed set of types with representation in both Scheme and Rust is used. These types are identified by a symbol or list in Scheme and map to a matching type in Rust.

### Primitive types

The following primitive types are supported:

| Type                         | Scheme Syntax | Rust type |
| ---------------------------- | ------------- | --------- |
| 8-bit unsigned integer       | `u8`          | `u8`      |
| 16-bit unsigned integer      | `u16`         | `u16`     |
| 32-bit unsigned integer      | `u32`         | `u32`     |
| 64-bit unsigned integer      | `u64`         | `u64`     |
| 8-bit signed integer         | `i8`          | `i8`      |
| 16-bit signed integer        | `i16`         | `i16`     |
| 32-bit signed integer        | `i32`         | `i32`     |
| 64-bit signed integer        | `i64`         | `i64`     |
| 32-bit floating point number | `f32`         | `f32`     |
| 64-bit floating point number | `f64`         | `f64`     |
| Boolean value                | `bool`        | `bool`    |
| String                       | `string`      | `String`  |

In Scheme, the allowed values for each of these types is the same as it would be in Rust, even though Scheme (or Guile specifically) does not enforce this limit. Boolean values must be written as `#t`/`#true` or `#f`/`#false`, other truthy values are not allowed in place of `#t`.

### Aggregate types

The following aggregate types are supported:

| Type   | Scheme Syntax         | Rust type               |
| ------ | --------------------- | ----------------------- |
| Tuple  | `(#type1 #type2 ...)` | `(#type1, #type2, ...)` |
| Vector | `(vec #type)`         | `Vec<#type>`            |

A **tuple** is a fixed-length sequence of up to eight elements of arbitrary types. When supplying a value to a variable with tuple type in Scheme, simply list the values for each element in order.

A **vector** is a list of variable length of entries of a single type. Values to vector type variables in scheme are simply lists which contain only elements of the vectors item type.

Vectors and tuples can be arbitrarily nested.

### Asset types

Certain assets can also be used as parameters or properties, when declaring the property with one of the following types:

| Type                       | Scheme Syntax       | Rust type                       |
| -------------------------- | ------------------- | ------------------------------- |
| [Node](nodes.md)           | `(node #node-type)` | `#NodeType`                     |
| [Node list](node_lists.md) | `node-list`         | `skylite_core::nodes::NodeList` |

Node variables must be declared with a `#node-type`, which should be a symbol containing the name of the node asset. When passing a value to a node type parameter, a list starting with the node type and followed by the arguments to the node's parameters should be used.

Example:
```scheme
; Declaration
(node my-node)

; Value
(my-node 1 2 3)
```

A node list variable simply has the type `node-list`. As a value, the a symbol containing the name of a node list asset should be used.
