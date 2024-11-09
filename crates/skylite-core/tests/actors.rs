use skylite_proc::actor_definition;

actor_definition! {
    use crate::project1::TestProject1;

    skylite_proc::asset_file!("./tests/test-project-1/project.scm", "testactor");

    #[skylite_proc::action("test1")]
    fn test1(actor: &mut Testactor, project: &mut TestProject1) {}

    #[skylite_proc::action("test2")]
    fn test2(actor: &mut Testactor, project: &mut TestProject1, val1: u8, val2: u8) {}
}
