use super::scorers::Scorer;
use crate::eval::Lang;
use crate::llm::prompt::make_prompt_from_task;
use crate::llm::PromptBuilder;

type PromptFactory = Box<dyn Fn(Lang) -> PromptBuilder + Send + Sync>;
type ScorerFactory = Box<dyn Fn(Lang, &str) -> Vec<Box<dyn Scorer>> + Send + Sync>;

pub struct BenchmarkSpec {
    pub id: &'static str,
    pub category: &'static str,
    pub make_prompt: PromptFactory,
    pub scorers: ScorerFactory,
}

impl BenchmarkSpec {
    pub fn from_tasks_auto(
        spec_file: &'static str,
        scorers_with_route: impl Fn(Lang, &str) -> Vec<Box<dyn Scorer>> + Send + Sync + 'static,
    ) -> Self {
        let (id, category) = infer_id_and_category(spec_file);
        let make_prompt = {
            Box::new(move |lang: Lang| {
                make_prompt_from_task(spec_file, id, lang).expect("missing tasks/<lang>.txt next to spec")
            }) as Box<dyn Fn(Lang) -> PromptBuilder + Send + Sync>
        };
        Self {
            id,
            category,
            make_prompt,
            scorers: Box::new(scorers_with_route),
        }
    }

    pub fn scorers_for(&self, lang: Lang, route_tag: &str) -> Vec<Box<dyn Scorer>> {
        (self.scorers)(lang, route_tag)
    }
}

pub fn infer_id_and_category(spec_file: &str) -> (&'static str, &'static str) {
    use std::path::Path;
    let p = Path::new(spec_file);
    let task = p
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|s| s.to_str())
        .expect("task dir");
    let cat = p
        .parent()
        .and_then(|d| d.parent())
        .and_then(|d| d.file_name())
        .and_then(|s| s.to_str())
        .expect("category dir");
    (
        Box::leak(task.to_string().into_boxed_str()),
        Box::leak(cat.to_string().into_boxed_str()),
    )
}
