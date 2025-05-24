use skylite_core::SkyliteProject;
use skylite_mock::{Call, MockTarget};
use skylite_proc::{node_definition, skylite_project};

skylite_proc::sequence_definition! {
    use super::SequenceTest;
    use super::FizzBuzz;

    skylite_proc::asset_file!("./tests/test-project-2/project.scm", "fizz-buzz-seq");
}

skylite_proc::node_definition! {
    use skylite_core::sequences::Sequencer;
    use super::FizzBuzz;
    use super::SequenceTest;

    skylite_proc::asset_file!("./tests/test-project-2/project.scm", "wrapper");

    skylite_proc::extra_properties! {
        pub sequencer: Sequencer<FizzBuzz>
    }

    #[skylite_proc::create_properties]
    fn create_properties() -> WrapperProperties {
        WrapperProperties {
            sequencer: Sequencer::new(super::FizzBuzzSeqHandle)
        }
    }

    #[skylite_proc::update]
    fn update(node: &mut Wrapper, _controls: &mut ProjectControls<SequenceTest>) {
        node.properties.sequencer.update(&mut node.static_nodes.content);
    }
}

node_definition! {
    use super::SequenceTest;
    use super::FizzBuzzScratch;

    skylite_proc::asset_file!("./tests/test-project-2/project.scm", "fizz-buzz");

    #[skylite_proc::create_properties]
    fn create_properties() -> FizzBuzzProperties {
        FizzBuzzProperties {
            counter: 0,
            status: String::new(),
            stop: false
        }
    }

    #[skylite_proc::update]
    fn update(node: &mut FizzBuzz, controls: &mut skylite_core::ProjectControls<SequenceTest>) {
        controls.get_target_instance_mut().log(&format!("Counter: {}, Status: {}", node.properties.counter, node.properties.status));
    }
}

node_definition! {
    use super::SequenceTest;

    skylite_proc::asset_file!("./tests/test-project-2/project.scm", "fizz-buzz-scratch");

    #[skylite_proc::create_properties]
    fn create_properties() -> FizzBuzzScratchProperties {
        FizzBuzzScratchProperties { check_counter: 0, is_fizz: false, is_buzz: false }
    }
}

skylite_project! {
    use skylite_mock::MockTarget;
    use super::{FizzBuzz, FizzBuzzProperties, FizzBuzzStaticNodes, FizzBuzzScratch, FizzBuzzScratchProperties, Wrapper};

    skylite_proc::target_type!(MockTarget);
    skylite_proc::project_file!("./tests/test-project-2/project.scm");
}

fn match_call(call: &Call, expected: &str) -> bool {
    if let Call::Log { msg } = call {
        msg.as_str() == expected
    } else {
        false
    }
}

#[test]
fn test_sequence() {
    let mut target = MockTarget::new();
    target.push_tag("test");
    let mut project = SequenceTest::new(target);
    for _ in 0..20 {
        project.update();
    }

    let target = project._private_target();
    let call_history = target.get_calls_by_tag("test");
    assert_eq!(call_history.len(), 20);
    assert!(match_call(&call_history[0], "Counter: 0, Status: "));
    assert!(match_call(&call_history[1], "Counter: 1, Status: "));
    assert!(match_call(&call_history[2], "Counter: 2, Status: "));
    assert!(match_call(&call_history[3], "Counter: 3, Status: fizz"));
    assert!(match_call(&call_history[4], "Counter: 4, Status: "));
    assert!(match_call(&call_history[5], "Counter: 5, Status: buzz"));
    assert!(match_call(&call_history[6], "Counter: 6, Status: fizz"));
    assert!(match_call(&call_history[7], "Counter: 7, Status: "));
    assert!(match_call(&call_history[8], "Counter: 8, Status: "));
    assert!(match_call(&call_history[9], "Counter: 9, Status: fizz"));
    assert!(match_call(&call_history[10], "Counter: 10, Status: buzz"));
    assert!(match_call(&call_history[11], "Counter: 11, Status: "));
    assert!(match_call(&call_history[12], "Counter: 12, Status: fizz"));
    assert!(match_call(&call_history[13], "Counter: 13, Status: "));
    assert!(match_call(&call_history[14], "Counter: 14, Status: "));
    assert!(match_call(
        &call_history[15],
        "Counter: 15, Status: fizzbuzz"
    ));
    assert!(match_call(&call_history[16], "Counter: 16, Status: "));
    assert!(match_call(&call_history[17], "Counter: 17, Status: "));
    assert!(match_call(&call_history[18], "Counter: 18, Status: fizz"));
    assert!(match_call(&call_history[19], "Counter: 19, Status: "));
}
