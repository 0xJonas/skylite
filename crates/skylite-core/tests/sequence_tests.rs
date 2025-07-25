use skylite_core::SkyliteProject;
use skylite_mock::{Call, MockTarget};
use skylite_proc::{node_definition, sequence_definition, skylite_project};

#[sequence_definition("./tests/test-project-2/project.scm", "fizz-buzz-seq")]
mod fizz_buzz_seq {
    use crate::fizz_buzz::FizzBuzz;
    use crate::project::SequenceTest;
}

#[node_definition("./tests/test-project-2/project.scm", "wrapper")]
mod wrapper {
    use skylite_core::sequences::Sequencer;
    use skylite_core::ProjectControls;

    use super::fizz_buzz::FizzBuzz;
    use crate::project::SequenceTest;

    pub(crate) struct Wrapper {
        #[skylite_proc::property]
        #[skylite_proc::node]
        pub content: FizzBuzz,
        sequencer: Sequencer<FizzBuzz>,
    }

    impl Wrapper {
        #[skylite_proc::new]
        pub(crate) fn new() -> Wrapper {
            Wrapper {
                content: FizzBuzz::new(),
                sequencer: Sequencer::new(crate::fizz_buzz_seq::FizzBuzzSeqHandle),
            }
        }

        #[skylite_proc::update]
        fn update(&mut self, _controls: &mut ProjectControls<SequenceTest>) {
            self.sequencer.update(&mut self.content);
        }
    }
}

#[node_definition("./tests/test-project-2/project.scm", "fizz-buzz")]
mod fizz_buzz {
    use super::fizz_buzz_scratch::FizzBuzzScratch;
    use crate::project::SequenceTest;

    pub(crate) struct FizzBuzz {
        #[skylite_proc::property]
        pub counter: i16,
        #[skylite_proc::property]
        pub status: String,
        #[skylite_proc::property]
        pub stop: bool,
        #[skylite_proc::property]
        #[skylite_proc::node]
        pub scratch: FizzBuzzScratch,
    }

    impl FizzBuzz {
        #[skylite_proc::new]
        pub(crate) fn new() -> FizzBuzz {
            FizzBuzz {
                counter: 0,
                status: String::new(),
                stop: false,
                scratch: FizzBuzzScratch::new(),
            }
        }

        #[skylite_proc::update]
        fn update(&self, controls: &mut skylite_core::ProjectControls<SequenceTest>) {
            controls.get_target_instance_mut().log(&format!(
                "Counter: {}, Status: {}",
                self.counter, self.status
            ));
        }
    }
}

#[node_definition("./tests/test-project-2/project.scm", "fizz-buzz-scratch")]
mod fizz_buzz_scratch {
    use crate::project::SequenceTest;

    pub(crate) struct FizzBuzzScratch {
        #[skylite_proc::property]
        pub check_counter: i16,
        #[skylite_proc::property]
        pub is_fizz: bool,
        #[skylite_proc::property]
        pub is_buzz: bool,
    }

    impl FizzBuzzScratch {
        #[skylite_proc::new]
        pub(crate) fn new() -> FizzBuzzScratch {
            FizzBuzzScratch {
                check_counter: 0,
                is_fizz: false,
                is_buzz: false,
            }
        }
    }
}

#[skylite_project("./tests/test-project-2/project.scm", MockTarget)]
mod project {
    use skylite_mock::MockTarget;

    use super::fizz_buzz::FizzBuzz;
    use super::fizz_buzz_scratch::FizzBuzzScratch;
    use super::wrapper::Wrapper;
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
    let mut project = project::SequenceTest::new(target);
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
