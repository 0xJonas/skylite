use skylite_mock::{Call, MockTarget};
use skylite_core::SkyliteProject;
use skylite_proc::skylite_project;

skylite_proc::actor_definition! {
    use skylite_core::{scenes::Scene, DrawContext, ProjectControls};
    use super::TestProject1;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "basic_actor_1");

    skylite_proc::properties! {
        pub tag: String
    }

    #[skylite_proc::create_properties]
    fn create_properties(tag: String) -> BasicActor1Properties {
        BasicActor1Properties { tag }
    }

    #[skylite_proc::action("action1")]
    fn action1(_actor: &mut BasicActor1, _scene: &mut dyn Scene<P=TestProject1>, controls: &mut ProjectControls<TestProject1>, param1: u8, param2: u8) {
        controls.target.log(&format!("basic_actor_1::action1({},{})", param1, param2));
    }

    #[skylite_proc::action("action2")]
    fn action2(_actor: &mut BasicActor1, _scene: &mut dyn Scene<P=TestProject1>, controls: &mut ProjectControls<TestProject1>, param1: bool) {
        controls.target.log(&format!("basic_actor_1::action2({})", param1));
    }

    #[skylite_proc::action("action3")]
    fn action3(_actor: &mut BasicActor1, _scene: &mut dyn Scene<P=TestProject1>, controls: &mut ProjectControls<TestProject1>) {
        controls.target.log("basic_actor_1::action3");
    }

    #[skylite_proc::pre_update]
    fn pre_update(actor: &BasicActor1, _scene: &mut dyn Scene<P=TestProject1>, controls: &mut ProjectControls<TestProject1>) {
        controls.target.push_tag(&actor.properties.tag);
        controls.target.log("basic_actor_1::pre_update");
    }

    #[skylite_proc::post_update]
    fn post_update(_actor: &BasicActor1, _scene: &mut dyn Scene<P=TestProject1>, controls: &mut ProjectControls<TestProject1>) {
        controls.target.log("basic_actor_1::post_update");
        controls.target.pop_tag();
    }

    #[skylite_proc::render]
    fn render(actor: &BasicActor1, ctx: &mut DrawContext<TestProject1>) {
        ctx.target.push_tag(&actor.properties.tag);
        ctx.target.log("basic_actor_1::render");
        ctx.target.pop_tag();
    }
}

skylite_proc::actor_definition! {
    use skylite_core::{scenes::Scene, ProjectControls};
    use super::TestProject1;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "basic_actor_2");

    skylite_proc::properties! {
        pub tag: String
    }

    #[skylite_proc::create_properties]
    fn create_properties(tag: String) -> BasicActor2Properties {
        BasicActor2Properties { tag }
    }

    #[skylite_proc::action("idle")]
    fn idle(actor: &mut BasicActor2, _scene: &mut dyn Scene<P=TestProject1>, controls: &mut ProjectControls<TestProject1>) {
        controls.target.push_tag(&actor.properties.tag);
        controls.target.log("basic_actor_2::idle");
        controls.target.pop_tag();
    }
}

skylite_proc::scene_definition! {
    use skylite_core::{ProjectControls, DrawContext};
    use super::TestProject1;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "basic_scene_1");

    skylite_proc::properties! {
        pub tag: String
    }

    #[skylite_proc::create_properties]
    fn create_properties(tag: String) -> BasicScene1Properties {
        BasicScene1Properties { tag }
    }

    #[skylite_proc::pre_update]
    fn pre_update(scene: &mut BasicScene1, controls: &mut ProjectControls<TestProject1>) {
        controls.target.push_tag(&scene.properties.tag);
        controls.target.log("basic_scene_1::pre_update");
    }

    #[skylite_proc::post_update]
    fn post_update(_scene: &mut BasicScene1, controls: &mut ProjectControls<TestProject1>) {
        controls.target.log("basic_scene_1::post_update");
        controls.target.pop_tag();
    }

    #[skylite_proc::pre_render]
    fn pre_render(scene: &BasicScene1, ctx: &mut DrawContext<TestProject1>) {
        ctx.target.push_tag(&scene.properties.tag);
        ctx.target.log("basic_scene_1::pre_render");
    }

    #[skylite_proc::post_render]
    fn post_render(_scene: &BasicScene1, ctx: &mut DrawContext<TestProject1>) {
        ctx.target.log("basic_scene_1::post_render");
        ctx.target.pop_tag();
    }
}

skylite_project! {
    use skylite_core::{SkyliteTarget, ProjectControls, DrawContext};
    use skylite_mock::MockTarget;

    use super::{BasicActor1, BasicActor2, BasicScene1};

    skylite_proc::project_file!("./tests/test-project-1/project.scm");

    skylite_proc::target_type!(MockTarget);

    #[skylite_proc::pre_update]
    fn pre_update(controls: &mut ProjectControls<TestProject1>) {
        controls.target.push_tag("root");
        controls.target.log("pre_update");
    }

    #[skylite_proc::post_update]
    fn post_update(controls: &mut ProjectControls<TestProject1>) {
        controls.target.log("post_update");
        controls.target.pop_tag();
    }

    #[skylite_proc::pre_render]
    fn pre_render(ctx: &mut DrawContext<TestProject1>) {
        ctx.target.push_tag("root");
        ctx.target.log("pre_render");
    }

    #[skylite_proc::post_render]
    fn post_render(ctx: &mut DrawContext<TestProject1>) {
        ctx.target.log("post_render");
        ctx.target.pop_tag();
    }
}

fn match_call(call: &Call, expected: &str) -> bool {
    if let Call::Log { msg } = call {
        msg.as_str() == expected
    } else {
        false
    }
}

#[test]
fn test_update_cycle() {
    let target = MockTarget::new();
    let mut project = TestProject1::new(target);

    project.update();
    let target = project._private_target();
    let calls = target.get_calls_by_tag("root");
    assert!(match_call(&calls[0], "pre_update"));
    assert!(match_call(&calls[1], "basic_scene_1::pre_update"));
    assert!(match_call(&calls[2], "basic_actor_1::pre_update"));
    assert!(match_call(&calls[3], "basic_actor_1::action3"));
    assert!(match_call(&calls[4], "basic_actor_1::post_update"));
    assert!(match_call(&calls[5], "basic_actor_2::idle"));
    assert!(match_call(&calls[6], "basic_actor_2::idle"));
    assert!(match_call(&calls[7], "basic_scene_1::post_update"));
    assert!(match_call(&calls[8], "post_update"));
    assert_eq!(calls.len(), 9);
}

#[test]
fn test_render_cycle() {
    let target = MockTarget::new();
    let mut project = TestProject1::new(target);

    project.render();
    let target = project._private_target();
    let calls = target.get_calls_by_tag("root");
    assert!(match_call(&calls[0], "pre_render"));
    assert!(match_call(&calls[1], "basic_scene_1::pre_render"));
    assert!(match_call(&calls[2], "basic_actor_1::render"));
    assert!(match_call(&calls[3], "basic_scene_1::post_render"));
    assert!(match_call(&calls[4], "post_render"));
    assert_eq!(calls.len(), 5);
}
