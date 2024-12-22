use skylite_proc::skylite_project;
use skylite_mock::MockTarget;
use skylite_core::SkyliteTarget;

skylite_proc::actor_definition! {
    use skylite_core::DrawContext;
    use skylite_core::actors::Actor;
    use skylite_core::scenes::Scene;
    use skylite_core::ProjectControls;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "test_actor");

    skylite_proc::properties! {
        pub x: i16,
        pub y: i16
    }

    #[skylite_proc::create_properties]
    fn create_properties_actor(x: i16, y: i16) -> TestActorProperties {
        TestActorProperties { x, y }
    }

    #[skylite_proc::action("move")]
    fn r#move(actor: &mut TestActor, _scene: &mut dyn Scene<P=TestProject1>, _controls: &mut ProjectControls<TestProject1>, dx: i8, dy: i8) {
        actor.properties.x += dx as i16;
        actor.properties.y += dy as i16;
    }

    #[skylite_proc::action("idle")]
    fn idle(_actor: &mut TestActor, _scene: &mut dyn Scene<P=TestProject1>, _controls: &mut ProjectControls<TestProject1>) {}

    #[skylite_proc::action("set-position")]
    fn set_position(actor: &mut TestActor, _scene: &mut dyn Scene<P=TestProject1>, _controls: &mut ProjectControls<TestProject1>, x: i16, y: i16) {
        actor.properties.x = x;
        actor.properties.y = y;

        // Change the current action by using a variant from the actor's action type.
        actor.set_action(TestActorActions::Idle {});
    }

    #[skylite_proc::render]
    fn render(_actor: &TestActor, _ctx: &DrawContext<TestProject1>) {
        // Draw something to the screen.
    }
}


skylite_proc::scene_definition! {

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "test_scene");

    skylite_proc::properties! {
        pub val1: bool,
        pub val2: u8
    }

    #[skylite_proc::create_properties]
    fn create_properties_scene(val1: bool, val2: u8) -> TestSceneProperties {
        TestSceneProperties { val1, val2 }
    }
}


skylite_project! {

    skylite_proc::project_file!("./tests/test-project-1/project.scm");

    skylite_proc::target_type!(MockTarget);

    #[skylite_proc::pre_update]
    fn pre_update(project: &mut TestProject1) {

    }
}
