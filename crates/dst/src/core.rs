use anyhow::Error;

pub trait TargetDriver<I> {
    type Observation;
    type Outcome;

    fn execute(&mut self, interaction: &I) -> Result<Self::Observation, Error>;

    fn finish(&mut self) -> Result<Self::Outcome, Error>;
}

pub trait Source {
    type Interaction;

    fn next_interaction(&mut self) -> Option<Self::Interaction>;
}

pub trait Properties<I, O> {
    fn observe(&mut self, interaction: &I, observation: &O) -> Result<(), Error>;

    fn finish(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub trait TestSuite {
    const NAME: &'static str;

    type Interaction;
    type Source: Source<Interaction = Self::Interaction>;
    type Target: TargetDriver<Self::Interaction>;
    type Properties: Properties<Self::Interaction, <Self::Target as TargetDriver<Self::Interaction>>::Observation>;

    fn build(&self) -> Result<TestRun<Self>, Error>
    where
        Self: Sized;
}

pub struct TestRun<S>
where
    S: TestSuite,
{
    pub source: S::Source,
    pub target: S::Target,
    pub properties: S::Properties,
}

pub fn run_test<S>(suite: S) -> Result<<S::Target as TargetDriver<S::Interaction>>::Outcome, Error>
where
    S: TestSuite,
    S::Interaction: Clone,
{
    let TestRun {
        mut source,
        mut target,
        mut properties,
    } = suite.build()?;

    while let Some(interaction) = source.next_interaction() {
        let observation = target.execute(&interaction)?;
        properties.observe(&interaction, &observation)?;
    }

    properties.finish()?;
    target.finish()
}
