use skylite_core::SkyliteProject;
use skylite_mock::{Call, MockTarget};
use skylite_proc::{node_definition, skylite_project};

#[node_definition("./tests/test-project-1/project.scm", "basic-node-1")]
mod basic_node_1 {
    use skylite_core::nodes::Node;
    use skylite_core::ProjectControls;

    use crate::basic_node_2::BasicNode2;
    use crate::project::TestProject1;
    use crate::z_order_node::ZOrderNode;

    pub(crate) struct BasicNode1 {
        #[skylite_proc::property]
        pub id: String,
        #[skylite_proc::node]
        sub1: BasicNode2,
        #[skylite_proc::node]
        sub2: ZOrderNode,
        #[skylite_proc::nodes]
        list: Vec<Box<dyn Node<P = TestProject1>>>,
    }

    impl BasicNode1 {
        #[skylite_proc::new]
        pub(crate) fn new_basic_node_1(id: String) -> BasicNode1 {
            BasicNode1 {
                id,
                sub1: BasicNode2::new_basic_node_2(String::from("sub1")),
                sub2: ZOrderNode::new_z_order_node(String::from("sub2"), -1),
                list: vec![
                    Box::new(BasicNode2::new_basic_node_2(String::from("list_item_1"))),
                    Box::new(ZOrderNode::new_z_order_node(String::from("list_item_2"), 2)),
                ],
            }
        }

        #[skylite_proc::pre_update]
        fn pre_update(&self, controls: &mut ProjectControls<TestProject1>) {
            controls.get_target_instance_mut().push_tag(&self.id);
            controls
                .get_target_instance_mut()
                .log("basic-node-1::pre_update");
        }

        #[skylite_proc::post_update]
        fn post_update(&self, controls: &mut ProjectControls<TestProject1>) {
            controls
                .get_target_instance_mut()
                .log("basic-node-1::post_update");
            controls.get_target_instance_mut().pop_tag();
        }
    }
}

#[node_definition("./tests/test-project-1/project.scm", "basic-node-2")]
mod basic_node_2 {
    use skylite_core::ProjectControls;

    use crate::project::TestProject1;

    pub(crate) struct BasicNode2 {
        #[skylite_proc::property]
        pub id: String,
    }

    impl BasicNode2 {
        #[skylite_proc::new]
        pub(crate) fn new_basic_node_2(id: String) -> BasicNode2 {
            BasicNode2 { id }
        }

        #[skylite_proc::update]
        fn update(&self, controls: &mut ProjectControls<TestProject1>) {
            controls.get_target_instance_mut().push_tag(&self.id);
            controls
                .get_target_instance_mut()
                .log("basic-node-2::update");
            controls.get_target_instance_mut().pop_tag();
        }
    }
}

#[node_definition("./tests/test-project-1/project.scm", "z-order-node")]
mod z_order_node {
    use skylite_core::{ProjectControls, RenderControls};

    use crate::project::TestProject1;

    pub(crate) struct ZOrderNode {
        #[skylite_proc::property]
        pub(crate) id: String,
        #[skylite_proc::property]
        pub(crate) z_order: i16,
    }

    impl ZOrderNode {
        #[skylite_proc::new]
        pub(crate) fn new_z_order_node(id: String, z_order: i16) -> ZOrderNode {
            ZOrderNode { id, z_order }
        }

        #[skylite_proc::z_order]
        fn z_order(&self) -> i32 {
            self.z_order as i32
        }

        #[skylite_proc::render]
        fn render(&self, ctx: &mut RenderControls<TestProject1>) {
            ctx.get_target_instance_mut().push_tag(&self.id);
            ctx.get_target_instance_mut()
                .log(&format!("z-order-node::render@{}", self.z_order));
            ctx.get_target_instance_mut().pop_tag();
        }

        #[skylite_proc::update]
        fn update(&self, controls: &mut ProjectControls<TestProject1>) {
            controls.get_target_instance_mut().push_tag(&self.id);
            controls
                .get_target_instance_mut()
                .log("z-order-node::update");
            controls.get_target_instance_mut().pop_tag();
        }
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
