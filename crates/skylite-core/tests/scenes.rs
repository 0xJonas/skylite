skylite_proc::scene_definition! {
    use crate::project1::*;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "test_scene");

    skylite_proc::properties! {
        pub val1: bool,
        pub val2: u8
    }

    #[skylite_proc::create_properties]
    fn create_properties(val1: bool, val2: u8) -> TestSceneProperties {
        TestSceneProperties { val1, val2 }
    }

}
