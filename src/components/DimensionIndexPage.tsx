import type { DimensionKind, TopDimensionRow } from "../types/dashboard";
import { TopList } from "./TopList";

interface DimensionIndexPageProps {
  agents: TopDimensionRow[];
  isLoading: boolean;
  models: TopDimensionRow[];
  onOpenDetail: (kind: DimensionKind, value: string) => void;
  projects: TopDimensionRow[];
  providers: TopDimensionRow[];
  sessions: TopDimensionRow[];
  workflows: TopDimensionRow[];
}

export function DimensionIndexPage({
  agents,
  isLoading,
  models,
  onOpenDetail,
  projects,
  providers,
  sessions,
  workflows,
}: DimensionIndexPageProps) {
  return (
    <section className="dimension-index">
      <section className="panel dimension-intro">
        <div>
          <p className="eyebrow">Dimension Analysis</p>
          <h2>按维度检查 Token 和调用质量</h2>
        </div>
        <p>
          从 Agent、模型、Provider、工作流、项目或会话排行进入详情，查看单一维度的趋势、关键指标和相关调用。
        </p>
      </section>

      <section className="dimension-list-grid">
        <TopList
          isLoading={isLoading}
          kind="agent"
          onRowClick={(value) => onOpenDetail("agent", value)}
          rows={agents}
          title="Agent"
        />
        <TopList
          isLoading={isLoading}
          kind="model"
          onRowClick={(value) => onOpenDetail("model", value)}
          rows={models}
          title="模型"
        />
        <TopList
          isLoading={isLoading}
          kind="provider"
          onRowClick={(value) => onOpenDetail("provider", value)}
          rows={providers}
          title="Provider"
        />
        <TopList
          isLoading={isLoading}
          kind="workflow"
          onRowClick={(value) => onOpenDetail("workflow", value)}
          rows={workflows}
          title="工作流"
        />
        <TopList
          isLoading={isLoading}
          kind="project"
          onRowClick={(value) => onOpenDetail("project", value)}
          rows={projects}
          title="项目"
        />
        <TopList
          isLoading={isLoading}
          kind="session"
          onRowClick={(value) => onOpenDetail("session", value)}
          rows={sessions}
          title="会话"
        />
      </section>
    </section>
  );
}
