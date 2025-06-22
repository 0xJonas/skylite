use skylite_core::SkyliteProject;
use skylite_mock::{Call, MockTarget};
use skylite_proc::{node_definition, skylite_project};

#[node_definition("./tests/test-project-1/project.scm", "basic-node-1")]
mod basic_node_1 {
    use skylite_core::ProjectControls;

    use super::basic_node_2::BasicNode2;
    use super::z_order_node::ZOrderNode;
    use crate::project::TestProject1;

    #[skylite_proc::create_properties]
    fn create_properties(id: String) -> BasicNode1Properties {
        BasicNode1Properties { id: id.to_owned() }
    }

    #[skylite_proc::pre_update]
    fn pre_update(node: &BasicNode1, controls: &mut ProjectControls<TestProject1>) {
        controls
            .get_target_instance_mut()
            .push_tag(&node.properties.id);
        controls
            .get_target_instance_mut()
            .log("basic-node-1::pre_update");
    }

    #[skylite_proc::post_update]
    fn post_update(_node: &BasicNode1, controls: &mut ProjectControls<TestProject1>) {
        controls
            .get_target_instance_mut()
            .log("basic-node-1::post_update");
        controls.get_target_instance_mut().pop_tag();
    }
}

#[node_definition("./tests/test-project-1/project.scm", "basic-node-2")]
mod basic_node_2 {
    use skylite_core::ProjectControls;

    use crate::project::TestProject1;

    #[skylite_proc::create_properties]
    fn create_properties(id: String) -> BasicNode2Properties {
        BasicNode2Properties { id: id.to_owned() }
    }

    #[skylite_proc::update]
    fn update(node: &BasicNode2, controls: &mut ProjectControls<TestProject1>) {
        controls
            .get_target_instance_mut()
            .push_tag(&node.properties.id);
        controls
            .get_target_instance_mut()
            .log("basic-node-2::update");
        controls.get_target_instance_mut().pop_tag();
    }
}

#[node_definition("./tests/test-project-1/project.scm", "z-order-node")]
mod z_order_node {
    use skylite_core::{ProjectControls, RenderControls};

    use crate::project::TestProject1;

    #[skylite_proc::create_properties]
    fn create_properties(id: String, z_order: i16) -> ZOrderNodeProperties {
        ZOrderNodeProperties {
            id: id.to_owned(),
            z_order,
        }
    }

    #[skylite_proc::z_order]
    fn z_order(node: &ZOrderNode) -> i32 {
        node.properties.z_order as i32
    }

    #[skylite_proc::render]
    fn render(node: &ZOrderNode, ctx: &mut RenderControls<TestProject1>) {
        ctx.get_target_instance_mut().push_tag(&node.properties.id);
        ctx.get_target_instance_mut()
            .log(&format!("z-order-node::render@{}", node.properties.z_order));
        ctx.get_target_instance_mut().pop_tag();
    }

    #[skylite_proc::update]
    fn update(node: &ZOrderNode, controls: &mut ProjectControls<TestProject1>) {
        controls
            .get_target_instance_mut()
            .push_tag(&node.properties.id);
        controls
            .get_target_instance_mut()
            .log("z-order-node::update");
        controls.get_target_instance_mut().pop_tag();
    }
}

#[skylite_project("./tests/test-project-1/project.scm", MockTarget)]
mod project {
    use skylite_core::nodes::SList;
    use skylite_core::{ProjectControls, RenderControls};
    use skylite_mock::MockTarget;

    use crate::basic_node_1::BasicNode1;
    use crate::basic_node_2::BasicNode2;

    #[skylite_proc::pre_update]
    fn pre_update(controls: &mut ProjectControls<TestProject1>) {
        controls.get_target_instance_mut().push_tag("root");
        controls.get_target_instance_mut().log("pre_update");
    }

    #[skylite_proc::post_update]
    fn post_update(controls: &mut ProjectControls<TestProject1>) {
        controls.get_target_instance_mut().log("post_update");
        controls.get_target_instance_mut().pop_tag();
    }

    #[skylite_proc::pre_render]
    fn pre_render(ctx: &mut RenderControls<TestProject1>) {
        ctx.get_target_instance_mut().push_tag("root");
        ctx.get_target_instance_mut().log("pre_render");
    }

    #[skylite_proc::post_render]
    fn post_render(ctx: &mut RenderControls<TestProject1>) {
        ctx.get_target_instance_mut().log("post_render");
        ctx.get_target_instance_mut().pop_tag();
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
    let mut project = project::TestProject1::new(target);

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
    let mut project = project::TestProject1::new(target);

    project.render();
    let target = project._private_target();
    let calls = target.get_calls_by_tag("root");
    assert_eq!(calls.len(), 4);
    assert!(match_call(&calls[0], "pre_render"));
    assert!(match_call(&calls[1], "z-order-node::render@-1"));
    assert!(match_call(&calls[2], "z-order-node::render@2"));
    assert!(match_call(&calls[3], "post_render"));
}
