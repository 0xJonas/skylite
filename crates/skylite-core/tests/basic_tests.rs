use skylite_core::SkyliteProject;
use skylite_mock::{Call, MockTarget};
use skylite_proc::{node_definition, skylite_project};

node_definition! {
    use skylite_core::ProjectControls;
    use super::TestProject1;
    use super::BasicNode2;
    use super::ZOrderNode;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "basic-node-1");

    #[skylite_proc::create_properties]
    fn create_properties(id: String) -> BasicNode1Properties {
        BasicNode1Properties { id: id.to_owned() }
    }

    #[skylite_proc::pre_update]
    fn pre_update(node: &BasicNode1, controls: &mut ProjectControls<TestProject1>) {
        controls.target.push_tag(&node.properties.id);
        controls.target.log("basic-node-1::pre_update");
    }

    #[skylite_proc::post_update]
    fn post_update(_node: &BasicNode1, controls: &mut ProjectControls<TestProject1>) {
        controls.target.log("basic-node-1::post_update");
        controls.target.pop_tag();
    }
}

node_definition! {
    use skylite_core::ProjectControls;
    use super::TestProject1;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "basic-node-2");

    #[skylite_proc::create_properties]
    fn create_properties(id: String) -> BasicNode2Properties {
        BasicNode2Properties { id: id.to_owned() }
    }

    #[skylite_proc::update]
    fn update(node: &BasicNode2, controls: &mut ProjectControls<TestProject1>) {
        controls.target.push_tag(&node.properties.id);
        controls.target.log("basic-node-2::update");
        controls.target.pop_tag();
    }
}

node_definition! {
    use skylite_core::{DrawContext, ProjectControls};
    use super::TestProject1;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "z-order-node");

    #[skylite_proc::create_properties]
    fn create_properties(id: String, z_order: i16) -> ZOrderNodeProperties {
        ZOrderNodeProperties { id: id.to_owned(), z_order }
    }

    #[skylite_proc::z_order]
    fn z_order(node: &ZOrderNode) -> i32 {
        node.properties.z_order as i32
    }

    #[skylite_proc::render]
    fn render(node: &ZOrderNode, ctx: &mut DrawContext<TestProject1>) {
        ctx.target.push_tag(&node.properties.id);
        ctx.target.log(&format!("z-order-node::render@{}", node.properties.z_order));
        ctx.target.pop_tag();
    }

    #[skylite_proc::update]
    fn update(node: &ZOrderNode, controls: &mut ProjectControls<TestProject1>) {
        controls.target.push_tag(&node.properties.id);
        controls.target.log("z-order-node::update");
        controls.target.pop_tag();
    }
}

skylite_project! {
    use skylite_mock::MockTarget;

    use super::{BasicNode1, BasicNode2};

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
    assert_eq!(calls.len(), 9);
    assert!(match_call(&calls[0], "pre_update"));
    assert!(match_call(&calls[1], "basic-node-1::pre_update"));
    assert!(match_call(&calls[2], "basic-node-2::update"));
    assert!(match_call(&calls[3], "z-order-node::update"));
    assert!(match_call(&calls[4], "basic-node-2::update"));
    assert!(match_call(&calls[5], "z-order-node::update"));
    assert!(match_call(&calls[6], "basic-node-1::post_update"));
    assert!(match_call(&calls[7], "basic-node-2::update"));
    assert!(match_call(&calls[8], "post_update"));
}

#[test]
fn test_render_cycle() {
    let target = MockTarget::new();
    let mut project = TestProject1::new(target);

    project.render();
    let target = project._private_target();
    let calls = target.get_calls_by_tag("root");
    assert_eq!(calls.len(), 4);
    assert!(match_call(&calls[0], "pre_render"));
    assert!(match_call(&calls[1], "z-order-node::render@-1"));
    assert!(match_call(&calls[2], "z-order-node::render@2"));
    assert!(match_call(&calls[3], "post_render"));
}
