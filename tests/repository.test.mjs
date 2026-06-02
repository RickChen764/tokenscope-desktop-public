import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const root = process.cwd();

function readProjectFile(path) {
  return readFileSync(join(root, path), "utf8");
}

function openSeededDatabase() {
  const db = new DatabaseSync(":memory:");
  runAllMigrations(db);
  db.exec(readProjectFile("src-tauri/sql/seed_demo.sql"));
  return db;
}

function runAllMigrations(db) {
  const migrations = readdirSync(join(root, "src-tauri/migrations"))
    .filter((name) => name.endsWith(".sql"))
    .sort();

  for (const migration of migrations) {
    db.exec(readProjectFile(join("src-tauri/migrations", migration)));
  }
}

test("initial migration creates the Phase 1 tables", () => {
  const db = new DatabaseSync(":memory:");

  db.exec(readProjectFile("src-tauri/migrations/0001_initial_schema.sql"));

  const tables = db
    .prepare(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .all()
    .map((row) => row.name);

  assert.deepEqual(tables, [
    "daily_usage_agg",
    "llm_call",
    "pricing_rule",
    "provider_config",
  ]);
});

test("seed data avoids raw prompt and response payload storage by default", () => {
  const db = openSeededDatabase();

  const rawPayloads = db
    .prepare(
      "SELECT COUNT(*) AS count FROM llm_call WHERE raw_response_json IS NOT NULL",
    )
    .get();
  const hashedCalls = db
    .prepare(
      "SELECT COUNT(*) AS count FROM llm_call WHERE request_hash IS NOT NULL AND response_hash IS NOT NULL",
    )
    .get();

  assert.equal(rawPayloads.count, 0);
  assert.ok(hashedCalls.count > 0);
});

test("dashboard summary query returns totals, rates, and top dimensions", () => {
  const db = openSeededDatabase();
  const query = readProjectFile("src-tauri/sql/dashboard_summary.sql");

  const today = db.prepare("SELECT date('now', 'localtime') AS value").get().value;
  const row = db.prepare(query).get(today, today);

  assert.equal(row.calls, 3);
  assert.equal(row.success_calls, 2);
  assert.equal(row.error_calls, 1);
  assert.equal(row.total_tokens, 17920);
  assert.equal(row.input_tokens, 13200);
  assert.equal(row.output_tokens, 4720);
  assert.equal(row.cached_input_tokens, 4200);
  assert.equal(row.reasoning_output_tokens, 680);
  assert.equal(row.error_rate, 1 / 3);
  assert.equal(row.top_agent_id, "researcher");
  assert.equal(row.top_model, "gpt-5-mini");
  assert.ok(row.estimated_cost_usd > 0);
  assert.equal(row.cost_currency, "USD");
  assert.ok(row.avg_latency_ms > 0);
});

test("dashboard summary query returns zero totals for an empty range", () => {
  const db = new DatabaseSync(":memory:");
  runAllMigrations(db);
  const query = readProjectFile("src-tauri/sql/dashboard_summary.sql");

  const row = db.prepare(query).get("1970-01-01", "1970-01-01");

  assert.equal(row.calls, 0);
  assert.equal(row.success_calls, 0);
  assert.equal(row.error_calls, 0);
  assert.equal(row.total_tokens, 0);
  assert.equal(row.error_rate, 0);
  assert.equal(row.cost_currency, "USD");
  assert.equal(row.avg_latency_ms, null);
  assert.equal(row.top_agent_id, null);
  assert.equal(row.top_model, null);
});

test("daily usage series groups tokens and cost by local date", () => {
  const db = openSeededDatabase();
  const query = readProjectFile("src-tauri/sql/daily_usage_series_total.sql");

  const row = db
    .prepare(query)
    .all("1970-01-01", "2999-12-31")
    .find((item) => item.calls === 3);

  assert.equal(row.total_tokens, 17920);
  assert.equal(row.input_tokens, 13200);
  assert.equal(row.output_tokens, 4720);
  assert.ok(row.estimated_cost_usd > 0);
  assert.equal(row.cost_currency, "USD");
});

test("top dimension queries rank providers, agents, models, workflows, projects, and sessions", () => {
  const db = openSeededDatabase();
  const from = "1970-01-01";
  const to = "2999-12-31";

  const topProvider = db
    .prepare(readProjectFile("src-tauri/sql/top_providers.sql"))
    .get(from, to, 1);
  const topAgent = db
    .prepare(readProjectFile("src-tauri/sql/top_agents.sql"))
    .get(from, to, 1);
  const topModel = db
    .prepare(readProjectFile("src-tauri/sql/top_models.sql"))
    .get(from, to, 1);
  const topWorkflow = db
    .prepare(readProjectFile("src-tauri/sql/top_workflows.sql"))
    .get(from, to, 1);
  const topProject = db
    .prepare(readProjectFile("src-tauri/sql/top_projects.sql"))
    .get(from, to, 1);
  const topSession = db
    .prepare(readProjectFile("src-tauri/sql/top_sessions.sql"))
    .get(from, to, 1);

  assert.equal(topProvider.dimension, "openai-compatible");
  assert.equal(topAgent.dimension, "researcher");
  assert.equal(topModel.dimension, "gpt-5-mini");
  assert.equal(topWorkflow.dimension, "report_generation");
  assert.equal(topProject.dimension, "demo_project");
  assert.equal(topSession.dimension, "sess_demo_001");
  assert.ok(topProvider.total_tokens > 0);
  assert.ok(topAgent.estimated_cost_usd > 0);
  assert.ok(topModel.total_tokens >= topWorkflow.total_tokens);
  assert.ok(topProject.total_tokens >= topSession.total_tokens);
});
