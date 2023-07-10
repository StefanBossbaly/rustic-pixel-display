mod home_assistant_tracker;
mod septa_tracker;

#[derive(Clone, Copy, Debug)]
pub enum UsefulnessVal {
    NotUseful,
    BarelyUseful,
    SomewhatUseful,
    Useful,
    VeryUseful,
    Essential,
}

pub trait Usefulness {
    fn usefulness(&self) -> UsefulnessVal;
}

pub trait StateProvider {
    type State: Usefulness;

    fn provide_state(&self) -> Self::State;
}
