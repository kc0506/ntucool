use anyhow::Result;
use clap::Args;

use crate::output::OutputFormat;

#[derive(Args)]
pub struct GradeArgs {
    /// Optional course filter. Omit to list every active course's grade.
    #[arg(long)]
    pub course: Option<i64>,
}

pub async fn run(args: GradeArgs, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let grades = cool_tools::grades::grades_get(&client, args.course).await?;
    let fmt = OutputFormat::from_flag(opts.json);

    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&grades)?),
        OutputFormat::Table => {
            if grades.is_empty() {
                println!("(no graded enrolments — courses may hide grades or have no graded work yet)");
                return Ok(());
            }
            for g in &grades {
                let name = g.course_name.as_deref().unwrap_or("(unknown)");
                let cur_score = g
                    .current_score
                    .map(|s| format!("{:.1}", s))
                    .unwrap_or_else(|| "—".to_string());
                let cur_grade = g.current_grade.as_deref().unwrap_or("—");
                let fin_score = g
                    .final_score
                    .map(|s| format!("{:.1}", s))
                    .unwrap_or_else(|| "—".to_string());
                let fin_grade = g.final_grade.as_deref().unwrap_or("—");
                println!(
                    "[{}] {}\n    current: {} ({})    final: {} ({})",
                    g.course_id, name, cur_score, cur_grade, fin_score, fin_grade
                );
            }
        }
    }
    Ok(())
}
