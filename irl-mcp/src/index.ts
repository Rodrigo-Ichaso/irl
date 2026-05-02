#!/usr/bin/env node
// IRL MCP Server — Intent Record Language
// Exposes evaluate_intent so any MCP-compatible agent checks with IRL before acting.

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";

const IRL_URL = process.env.IRL_URL ?? "http://192.168.0.8:8800";
const AGENT_ID = process.env.IRL_AGENT_ID ?? "claude-code";
const AGENT_TRUST = process.env.IRL_AGENT_TRUST ?? "medium";

const EvaluateSchema = z.object({
  operation_type: z.enum(["read", "write", "delete", "execute", "network", "auth"]),
  target_resource: z.string().describe("What is being affected (file path, DB table, endpoint, etc.)"),
  target_environment: z.enum(["local", "staging", "production"]),
  goal: z.string().describe("Why this action is needed"),
  verified: z.boolean().describe("Did you verify this is correct before asking?"),
  reversible: z.boolean().describe("Can this action be undone?"),
  alternatives: z.array(z.string()).optional().describe("Other approaches considered"),
  data_loss_risk: z.enum(["none", "partial", "total"]).optional(),
  affects_backups: z.boolean().optional(),
  rollback_plan: z.boolean().optional(),
});

type EvaluateInput = z.infer<typeof EvaluateSchema>;

async function callIRL(input: EvaluateInput): Promise<object> {
  const body = {
    irl_version: "0.1",
    agent: {
      id: AGENT_ID,
      model: "claude-sonnet-4-6",
      trust_level: AGENT_TRUST,
    },
    operation: {
      type: input.operation_type,
      target_resource: input.target_resource,
      target_environment: input.target_environment,
    },
    rationale: {
      stated_goal: input.goal,
      verified: input.verified,
      alternatives_considered: input.alternatives ?? [],
    },
    consequences: {
      reversible: input.reversible,
      data_loss_risk: input.data_loss_risk ?? "none",
      affects_backups: input.affects_backups ?? false,
      rollback_plan: input.rollback_plan ?? false,
    },
  };

  const res = await fetch(`${IRL_URL}/evaluate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

  return await res.json();
}

function formatVerdict(verdict: Record<string, unknown>): string {
  const decision = verdict.decision as string;
  const risk = verdict.risk as Record<string, unknown>;
  const score = risk?.score;
  const level = risk?.level;
  const reasons = (risk?.reasons as string[])?.join(", ") || "none";

  const lines = [
    `VERDICT: ${decision}`,
    `RISK: ${level} (${score}/100)`,
    `REASONS: ${reasons}`,
    `POLICY: ${verdict.policy}`,
    `EXPLANATION: ${verdict.reason}`,
    `VERDICT_ID: ${verdict.verdict_id}`,
  ];

  if (decision === "DENY") {
    lines.push("\n⛔ DO NOT proceed. This action is blocked by IRL.");
  } else if (decision === "GATE") {
    lines.push("\n⏸ PAUSE. Human approval required before proceeding.");
    lines.push("Notify the operator and wait for APPROVE response.");
  } else if (decision === "LOG+ALLOW") {
    lines.push("\n⚠️ Proceed with caution. Action is logged.");
  } else {
    lines.push("\n✅ Proceed. Action is approved.");
  }

  return lines.join("\n");
}

const server = new Server(
  { name: "irl-mcp", version: "0.1.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "evaluate_intent",
      description:
        "REQUIRED before any risky action: file deletion, production writes, " +
        "executing scripts, network calls to external services, or auth changes. " +
        "Submit your intent to IRL and receive ALLOW / LOG+ALLOW / GATE / DENY. " +
        "If DENY → stop. If GATE → wait for human. If ALLOW → proceed.",
      inputSchema: {
        type: "object",
        properties: {
          operation_type: {
            type: "string",
            enum: ["read", "write", "delete", "execute", "network", "auth"],
            description: "Type of operation",
          },
          target_resource: {
            type: "string",
            description: "What is being affected (file path, DB table, endpoint, service, etc.)",
          },
          target_environment: {
            type: "string",
            enum: ["local", "staging", "production"],
            description: "Environment where the action happens",
          },
          goal: {
            type: "string",
            description: "Why this action is needed",
          },
          verified: {
            type: "boolean",
            description: "Did you verify this is the correct action?",
          },
          reversible: {
            type: "boolean",
            description: "Can this action be undone?",
          },
          alternatives: {
            type: "array",
            items: { type: "string" },
            description: "Other approaches you considered",
          },
          data_loss_risk: {
            type: "string",
            enum: ["none", "partial", "total"],
            description: "Risk of data loss",
          },
          affects_backups: {
            type: "boolean",
            description: "Does this action affect backup systems?",
          },
          rollback_plan: {
            type: "boolean",
            description: "Do you have a rollback plan?",
          },
        },
        required: ["operation_type", "target_resource", "target_environment", "goal", "verified", "reversible"],
      },
    },
  ],
}));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  if (req.params.name !== "evaluate_intent") {
    return { content: [{ type: "text", text: "Unknown tool" }], isError: true };
  }

  try {
    const input = EvaluateSchema.parse(req.params.arguments);
    const verdict = await callIRL(input);
    return {
      content: [{ type: "text", text: formatVerdict(verdict as Record<string, unknown>) }],
    };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return {
      content: [{ type: "text", text: `IRL evaluation failed: ${msg}` }],
      isError: true,
    };
  }
});

const transport = new StdioServerTransport();
await server.connect(transport);
