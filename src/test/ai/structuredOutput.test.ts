import { describe, expect, it } from 'vitest';

import { parsePlanResponse } from '@/lib/ai/agentPlanner';
import { parseReview } from '@/lib/ai/agentReviewer';
import { parseCompletionResponse } from '@/lib/ai/structuredOutput';

describe('planner structured output parsing', () => {
  it('parses prose-wrapped fenced JSON with object-form steps', () => {
    const text = `I will first outline the approach.\n\n\`\`\`JSON
    {
      "plan": {
        "description": "Investigate and repair the service",
        "steps": [
          { "description": "Check service status" },
          { "title": "Inspect recent logs" },
          { "task": "Restart the failing unit" }
        ]
      }
    }
    \`\`\``;

    expect(parsePlanResponse(text)).toEqual({
      description: 'Investigate and repair the service',
      steps: [
        { description: 'Check service status', status: 'pending' },
        { description: 'Inspect recent logs', status: 'pending' },
        { description: 'Restart the failing unit', status: 'pending' },
      ],
    });
  });

  it('parses balanced JSON embedded in prose and tolerates trailing commas', () => {
    const text = `Plan ready:\n{
      "plan": {
        "description": "Recover the deployment",
        "steps": [
          "Read the deployment manifest",
          "Roll out the fixed image",
        ],
      }
    }\nProceed once approved.`;

    expect(parsePlanResponse(text)).toEqual({
      description: 'Recover the deployment',
      steps: [
        { description: 'Read the deployment manifest', status: 'pending' },
        { description: 'Roll out the fixed image', status: 'pending' },
      ],
    });
  });

  it('splits string-based steps into ordered plan items', () => {
    const text = JSON.stringify({
      plan: {
        description: 'Follow a short checklist',
        steps: '1. Check disk\n2. Clear tmp\n- Verify free space',
      },
    });

    expect(parsePlanResponse(text)).toEqual({
      description: 'Follow a short checklist',
      steps: [
        { description: 'Check disk', status: 'pending' },
        { description: 'Clear tmp', status: 'pending' },
        { description: 'Verify free space', status: 'pending' },
      ],
    });
  });
});

describe('reviewer structured output parsing', () => {
  it('parses top-level review drift with synonym keys and camelCase boolean', () => {
    const text = `
    {
      "assessment": "needs revision",
      "summary": "The agent skipped verification.",
      "recommendation": "Re-run the final verification command",
      "shouldContinue": true
    }
    `;

    expect(parseReview(text)).toEqual({
      assessment: 'needs_correction',
      findings: 'The agent skipped verification.',
      suggestions: ['Re-run the final verification command'],
      shouldContinue: true,
    });
  });

  it('parses fenced JSON review payloads with string suggestions and inferred stop on critical issues', () => {
    const text = `\`\`\`json
    {
      "review": {
        "assessment": "critical",
        "findings": "A destructive command targeted the wrong path.",
        "suggestions": "Stop execution; confirm the target directory before retrying."
      }
    }
    \`\`\``;

    expect(parseReview(text)).toEqual({
      assessment: 'critical_issue',
      findings: 'A destructive command targeted the wrong path.',
      suggestions: ['Stop execution', 'confirm the target directory before retrying.'],
      shouldContinue: false,
    });
  });

  it('tolerates prose around result-wrapped review JSON', () => {
    const text = `Reviewer output follows:\n{
      "result": {
        "review": {
          "assessment": "on_track",
          "findings": "Progress is consistent.",
          "suggestions": ["Keep the current plan"],
          "should_continue": "true"
        }
      }
    }\nEnd of review.`;

    expect(parseReview(text)).toEqual({
      assessment: 'on_track',
      findings: 'Progress is consistent.',
      suggestions: ['Keep the current plan'],
      shouldContinue: true,
    });
  });
});

describe('completion structured output parsing', () => {
  it('parses prose-wrapped completion JSON with trailing commas and object details', () => {
    const text = `Final status:\n\`\`\`json
    {
      "status": "success",
      "summary": "Service is healthy again",
      "details": { "checks": ["systemctl status", "curl /healthz"] },
    }
    \`\`\``;

    expect(parseCompletionResponse(text)).toEqual({
      status: 'completed',
      summary: 'Service is healthy again',
      details: JSON.stringify({ checks: ['systemctl status', 'curl /healthz'] }, null, 2),
    });
  });

  it('parses result-wrapped failed completions with synonym status', () => {
    const text = JSON.stringify({
      result: {
        outcome: 'blocked',
        message: 'Deployment could not continue',
        detail: 'Missing credentials',
      },
    });

    expect(parseCompletionResponse(text)).toEqual({
      status: 'failed',
      summary: 'Deployment could not continue',
      details: 'Missing credentials',
    });
  });
});