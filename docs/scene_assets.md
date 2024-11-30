# Scene Asset File Format

Scene assets are Scheme files which describe the contents and parameters of a scene. Each scene asset must have a matching [`scene_definition!`](scene_definition.md) block in the Rust code to provide the required types and implementations for the scene to be used by the engine.

The top level of a scene asset is an association list ('alist') with the following keys:

```scheme
'(
  ; List of named actors.
  (actors . (...))

  ; List of extras
  (extras . (...))

  ; Parameter declarations
  (parameters . (...)))
```

## Named actors and extras

A scene contains two lists of actors, **named actors** and **extras**. They both contain actor instances, but each list has special capabilities and limitations that make it suitable for different use cases.

- **Named actors** are actors which have a unique identifier within the scene, which enables them to be retrieved directly through that identifier, as well as being referenced in plays. The list of named actors is defined through the scene asset and cannot be changed at runtime. Named actors are ideal for all game objects that are known to be in the scene at build time, like player and non-player characters, level obstacles or loading zone triggers, as well as everything that should at some point be controlled by a play.

  Named actors are defined by a pair consisting of an identifier as a symbol and an actor instance. The actor instance starts with the name of an [actor asset file](actor_assets.md), followed by the actor's arguments:

  ```scheme
  ; Defines a actor instance named 'obj1' of actor type 'my_actor' with arguments 5 and 10.
  (obj1 . (my_actor 5 10))
  ```

  The arguments must match the order and type of that actor's parameters.

- **Extras** do not have an identifier, but instead, the list of extras can be modified at runtime. This makes extras ideal for things like projectiles, visual effects or any other game object that is only created dynamically. Since extras cannot be identified by name, they cannot be referenced in plays. The initial list of extras is defined as a list of actor instances, where each instance consists of the name of an actor asset and arguments:

  ```scheme
  ; Defines an actor instance of type 'my_actor' with arguments 15 and 20.
  (my_actor 15 20)
  ```

## Parameters

A scene can use parameters to initialize its properties and perform initial changes to its actor lists when it is instantiated. The content of the `parameters` key should be a list of [variable declarations](variables_and_types.md). The declared parameters are used when a scene is instantiated from Rust code or from other asset files.

## Example

```scheme
'(
  ; For the named actors, define two instance of test_actor,
  ; with names actor-1 and actor-2
  (actors .
    ((actor-1 . (test_actor 10 10))
     (actor-2 . (test_actor 20 20))))

  ; For the extras, define a third instance of test_actor.
  (extras .
    ((test_actor 30 30)))

  ; Define two parameters, one bool and one u8
  (parameters . ((param1 bool) (param2 u8))))
```
