# Sequences

A sequence is a small program that can manipulate and animate the properties of a [node](nodes.md). It can set and modify fields on the node or on child nodes, do basic control flow, and invoke custom functions.

## Syntax

The top-level of a sequence asset is an alist with the following keys:

```scheme
'((node . ...)
  (subs . (...))
  (script . <script>))
```

The `node` key specifies the name of the [node asset](nodes.md) that the sequence is targeting. Each sequence is always specific to a single node type, which is used to resolve the field names used in the subroutines and script.

The `subs` key contains a list of subroutines that can be called from the main script or from each other. The value for this key is another alist that maps subroutine names to scripts:

```scheme
'((subs .
    ((sub1 . <script1>)
     (sub2 . <script2>)))
  ; ...
  )
```

The `script` key contains the main script of the sequence. The script runs until either the end of the `script` is reached, or a `return` instruction is run.

### Script Syntax

The value of the `script` key, as well as the values for `subs` alist are scripts. A script is a list of instructions, and labels that can be used by certain control-flow-changing instructions:

```scheme
'(start ; global label
    (wait 1)  ; 'wait' instruction
    (branch my_field +)
    (wait 1)
  + (jump start)  ; local label '+' and 'jump' instruction
  )
```

There are two kinds of labels:

- Local labels start with either a `-` or a `+`. When the name of a local label is used in a control flow instruction, the target label is found by searching either backwards (if the label starts with `-`) or forwards (if the label starts with `+`), until a matching label is found. This means that multiple local labels with the same name can be used throughout a script; each instruction that uses that name will jump to the closest one either backwards or forwards. Note that the control flow instruction itself is **not** checked for the label:

  ```scheme
  '(- (wait 1)
    - (branch my_field -)      ; Branches to the previous instruction
      (branch (! my_field) -)  ; Branches to the previous instruction
      )
  ```

- Global label start with any character (that would be valid at the start of a Scheme symbol) other than `-` or `+`. Global labels must have unique names, and instructions using the name of a global label will search the entire script for that label.

If a label of either kind cannot be resolved, it will result in a compile-time error. The main script, as well as each subroutine, have their own label namespaces, so it is not possible to jump into, out of, or between subroutines using labels.

### Instructions

Following is the complete list of instructions allowed inside a script or sub:

| Instruction                | Syntax                         | Description                                          |
| -------------------------- | ------------------------------ | ---------------------------------------------------- |
| Set Field                  | `(set <field> <value>)`        | Sets a field to a new value.                         |
| Modify Field               | `(modify <field> <delta>)`     | Adds a value to a field.                             |
| Branch                     | `(branch <condition> <label>)` | Jumps to a label if a condition is met.              |
| Jump                       | `(jump <label>)`               | Unconditionally jumps to a label.                    |
| Call Subroutine            | `(call <sub>)`                 | Starts running a subroutine.                         |
| Return                     | `(return)`                     | Return from a subroutine or the main script.         |
| Wait                       | `(wait <updates>)`             | Waits the given number of updates.                   |
| Run Custom Operation       | `(run-custom <op>)`            | Run custom code.                                     |
| Branch On Custom Condition | `(branch-custom <condition>)`  | Run a custom function and branch based on the result |

The `<condition>` for the `branch` instructions should be one of the following:

| Condition                         | Syntax                                        | Restrictions on field types                                  |
| --------------------------------- | --------------------------------------------- | ------------------------------------------------------------ |
| If true                           | `<field>`, `(<field>)`                        | `bool` only                                                  |
| If false                          | `(! <field>)`                                 | `bool` only                                                  |
| If equal to value                 | `(= <field> <value>)`, `(== <field> <value>)` | any [primitive type](variables_and_types.md#primitive-types) |
| If not equal to value             | `(!= <field> <value>)`                        | any [primitive type](variables_and_types.md#primitive-types) |
| If less than value                | `(< <field> <value>)`                         | numeric primitive types                                      |
| If greater than value             | `(> <field> <value>)`                         | numeric primitive types                                      |
| If less than or equal to value    | `(<= <field> <value>)`                        | numeric primitive types                                      |
| If greater than or equal to value | `(>= <field> <value>)`                        | numeric primitive types                                      |

`<field>` is the name of any [property](nodes.md#node-struct) on the target node. If the property is another node, properties of that node can be accessed by using a dot: `child-node.value`. The fields names used are converted to _lower_snake_case_ before they are resolved to the actual fields on the node. `<value>` should be a Scheme value of the appropriate type.

The `set` instructions can only be used on fields with [primitive types](variables_and_types.md#primitive-types). The `modify` instruction only works on numeric primitive types.

Calling `return` from a subroutine transfers control back to the calling subroutine or main script. Calling `return` from the main script ends the sequence.

`wait` will suspend sequence execution for the given number of calls to `Sequencer::update`. A `(wait 0)` instructions can be used as a no-op.

The implementations for the custom operations and conditions for the `run-custom` and `branch-custom` instructions must be given in the `#[sequence_definition(...)]`.

## Sequence Definition

For each sequence asset, there should be a sequence definition in Rust code. A sequence definition determines the location where the `SequenceHandle` is placed for a given sequence, as well as implement the logic for custom operations and conditions.

A sequence definition is created using the `#[sequence_definition()]` attribute on a `mod`:

```rust
#[sequence_definition("./project/project.scm", "my-sequence")]
mod myseq {
    // ...
}
```

The arguments to the `#[sequence_definition()]` are the path to the [main project file](project_files.md#main-project-file), as well as the name of the sequence asset, similar to [`#[node_definition(...)]`](nodes.md#node_definition).

Within a sequence definition, there are two attributes that mark special functions, which serve as the implementations to the custom operations and conditions used in the sequence. These attributes must be given with the `skylite_proc::`-prefix, regardless of imports:

- `#[skylite_proc::custom_op("<name>")]`: Marks the implementation of the custom operation `<name>`. The marked function should have the following signature:
  ```rust
  #[skylite_proc::custom_op("my-op")]
  fn my_op(node: &mut MyNode) { /* ... */ }
  ```
- `#[skylite_proc::custom_condition("<name>")]`: Marks the implementation of the custom condition `<name>`. The marked function should have the following signature:
  ```rust
  #[skylite_proc::custom_condition("my-condition")]
  fn my_condition(node: &MyNode) -> bool { /* ... */ }
  ```

The actual name of these functions are not significant. There must be an implementation for each custom operation and condition, otherwise it will be a compile-time error.
