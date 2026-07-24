'use strict';

const { evaluateIssue, parseSections } = require('../quality/issue_quality_policy.cjs');

const POLICY_VERSION = 1;
const MANAGED_COMMENT_MARKER = '<!-- oxideterm-maintenance-automation:v1 -->';

const AUTOMATION_LABELS = Object.freeze({
  candidate: 'automation:candidate',
  running: 'automation:running',
  botPullRequest: 'automation:bot-pr',
  needsHuman: 'automation:needs-human',
  windowsValidation: 'automation:windows-validation',
});

// Automation implementation and credential-bearing control planes cannot edit themselves.
const PROTECTED_PATH_PREFIXES = Object.freeze([
  '.github/actions/',
  '.github/workflows/',
  'scripts/automation/',
]);

const PROTECTED_PATHS = new Set([
  '.github/dependabot.yml',
  '.github/CODEOWNERS',
  'SECURITY.md',
]);

// These paths can be analyzed automatically, but implementation remains a human decision.
const HUMAN_REVIEW_PATH_PREFIXES = Object.freeze([
  '.github/release-notes/',
  'scripts/release/',
  'crates/oxideterm-cloud-sync/',
  'crates/oxideterm-gpui-cloud-sync/',
  'crates/oxideterm-secret-store/',
  'crates/oxideterm-update/',
  'crates/oxideterm-plugin-host-api/src/capabilities',
  'crates/oxideterm-plugin-host-api/src/secrets',
  'crates/oxideterm-connections/src/secret',
  'crates/oxideterm-network-proxy/src/credentials',
]);

const SENSITIVE_SIGNAL_PATTERNS = Object.freeze([
  {
    code: 'credential_or_secret_boundary',
    pattern: /\b(?:credential|password|passphrase|private key|secret|access token|api key)\b|凭据|密码|口令|私钥|密钥|令牌/iu,
  },
  {
    code: 'authentication_boundary',
    pattern: /\b(?:authentication|authorization|oauth|login)\b|身份验证|认证|授权|登录/iu,
  },
  {
    code: 'cloud_sync_boundary',
    pattern: /\bcloud[\s-]?sync\b|云同步/iu,
  },
  {
    code: 'release_or_update_boundary',
    pattern: /\b(?:release|updater|code signing|certificate|notari[sz]ation)\b|发布|更新器|代码签名|证书|公证/iu,
  },
  {
    code: 'destructive_data_boundary',
    pattern: /\b(?:delete|overwrite|migration|data loss)\b|删除|覆盖|迁移|数据丢失/iu,
  },
  {
    code: 'plugin_permission_boundary',
    pattern: /\bplugin\b.{0,32}\b(?:permission|capabilit(?:y|ies))\b|插件.{0,16}(?:权限|能力)/isu,
  },
]);

const VALIDATION_SIGNAL_PATTERNS = Object.freeze([
  {
    code: 'graphics_or_driver_validation',
    pattern: /\b(?:gpu|graphics|driver|directx|d3d|metal|vulkan|opengl)\b|显卡|图形驱动|驱动程序/iu,
  },
  {
    code: 'encoding_validation',
    pattern: /\b(?:encoding|big5|gbk|mojibake)\b|编码|乱码/iu,
  },
  {
    code: 'server_specific_validation',
    pattern: /\b(?:openssh|dropbear|ssh server|sftp server|vnc server|rdp server)\b|服务端|服务器兼容/iu,
  },
  {
    code: 'display_scale_validation',
    pattern: /\b(?:dpi|display scale|scaling)\b|缩放比例|显示缩放/iu,
  },
]);

const CREDENTIAL_PATTERNS = Object.freeze([
  { code: 'private_key_material', pattern: /-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----/u },
  { code: 'github_token', pattern: /\bgh[pousr]_[A-Za-z0-9_]{20,}\b/u },
  { code: 'openai_api_key', pattern: /\bsk-[A-Za-z0-9_-]{20,}\b/u },
]);

function normalizeLabels(labels) {
  return (labels || []).map((label) =>
    typeof label === 'string' ? label : label?.name
  ).filter(Boolean);
}

function meaningfulCharacterCount(value) {
  return Array.from(String(value || '').replace(/[\p{P}\p{S}\s]/gu, '')).length;
}

function issueCategory(labels) {
  const labelSet = new Set(labels);
  if (labelSet.has('compatibility')) return 'compatibility';
  if (labelSet.has('bug')) return 'bug';
  if (labelSet.has('enhancement') || labelSet.has('feature-request')) return 'enhancement';
  if (labelSet.has('question')) return 'question';
  return 'unclassified';
}

function platformSignals(body) {
  const sections = parseSections(body);
  const platformText = [
    sections.get('platform / 平台'),
    sections.get('client platform / 客户端平台'),
  ].filter(Boolean).join('\n') || body;
  const platforms = [];

  if (/\bwindows\b|\bwin(?:10|11)\b|微软系统/iu.test(platformText)) {
    platforms.push('windows');
  }
  if (/\bmac\s?os\b|\bos\s?x\b|苹果系统/iu.test(platformText)) {
    platforms.push('macos');
  }
  if (/\blinux\b|\bubuntu\b|\bdebian\b|\bfedora\b|\barch\b|统信|麒麟/iu.test(platformText)) {
    platforms.push('linux');
  }
  return platforms;
}

function matchingSignalCodes(text, definitions) {
  return definitions
    .filter(({ pattern }) => pattern.test(text))
    .map(({ code }) => code);
}

function narrativeTextForRisk(title, body) {
  const sections = parseSections(body);
  const narrativeHeadings = [
    'summary / 简述',
    'steps to reproduce / 复现步骤',
    'expected vs actual / 预期与实际',
    'logs or screenshots / 日志或截图',
    'additional environment details / 其他相关环境信息',
    'problem or use case / 问题或使用场景',
    'proposed solution / 期望方案',
    'error message or behavior / 错误信息或现象',
    'working client comparison / 可正常连接的客户端对比',
  ];
  const narrative = narrativeHeadings
    .map((heading) => sections.get(heading))
    .filter(Boolean);
  // Unknown templates still receive title analysis without scanning checklists or metadata fields.
  return [title, ...narrative].join('\n');
}

function hasUsefulReproduction(body) {
  const sections = parseSections(body);
  const reproduction = sections.get('steps to reproduce / 复现步骤') || '';
  const hasArtifact = /```|!\[[^\]]*\]\(|<img\b|\b(?:panic|crash|stacktrace|failed|exception)\b/iu.test(body);
  return meaningfulCharacterCount(reproduction) >= 12 || hasArtifact;
}

function recommendedLabelsForRoute(route, platforms) {
  const labels = [];
  if (route === 'candidate_for_agent') {
    labels.push(AUTOMATION_LABELS.candidate);
  } else if (route === 'needs_human') {
    labels.push(AUTOMATION_LABELS.needsHuman);
  }
  if (platforms.length === 1 && platforms[0] === 'windows') {
    labels.push(AUTOMATION_LABELS.windowsValidation);
  }
  return labels;
}

function analyzeIssue(issue) {
  const labels = normalizeLabels(issue.labels);
  const labelSet = new Set(labels);
  const category = issueCategory(labels);
  const body = String(issue.body || '');
  // REST uses lowercase states while GraphQL clients commonly return uppercase values.
  const issueState = String(issue.state || '').toLocaleLowerCase('en-US');
  const analysisText = narrativeTextForRisk(String(issue.title || ''), body);
  const platforms = platformSignals(body);
  const sensitiveSignals = matchingSignalCodes(analysisText, SENSITIVE_SIGNAL_PATTERNS);
  const validationSignals = matchingSignalCodes(analysisText, VALIDATION_SIGNAL_PATTERNS);
  const qualityReport = evaluateIssue({
    title: String(issue.title || ''),
    body,
    labels,
    releasedVersions: [],
  });
  const qualityBlocked = !labelSet.has('quality-check-exempt')
    && (
      labelSet.has('incomplete')
      || qualityReport.blockingFindings.length > 0
    );
  const usefulReproduction = hasUsefulReproduction(body);

  let route = 'observe_only';
  let confidence = category === 'unclassified' ? 'low' : 'medium';
  const reasons = [];

  if (issueState !== 'open') {
    reasons.push('issue_not_open');
  } else if (qualityBlocked) {
    route = 'blocked_by_quality_gate';
    reasons.push('quality_gate_blocking');
  } else if (sensitiveSignals.length > 0) {
    route = 'needs_human';
    reasons.push(...sensitiveSignals);
  } else if (
    category === 'compatibility'
    || validationSignals.length > 0
    || (platforms.length === 1 && platforms[0] === 'windows')
  ) {
    route = 'needs_human';
    reasons.push(
      ...(category === 'compatibility' ? ['compatibility_report'] : []),
      ...validationSignals,
      ...(platforms.length === 1 && platforms[0] === 'windows'
        ? ['windows_only_validation']
        : [])
    );
  } else if (category === 'bug' && usefulReproduction) {
    route = 'candidate_for_agent';
    confidence = 'high';
    reasons.push('bounded_bug_with_reproduction');
  } else if (category === 'enhancement') {
    route = 'needs_human';
    reasons.push('product_decision_required');
  } else if (category === 'bug') {
    reasons.push('reproduction_not_proven');
  } else {
    reasons.push('automatic_route_not_proven');
  }

  // Reports intentionally contain only derived codes, never the raw issue body or logs.
  return {
    schemaVersion: 1,
    policyVersion: POLICY_VERSION,
    issue: {
      number: Number(issue.number),
      url: String(issue.html_url || ''),
    },
    category,
    platforms,
    route,
    confidence,
    reasons: [...new Set(reasons)],
    qualityFindingCodes: [
      ...qualityReport.blockingFindings,
      ...qualityReport.reviewFindings,
    ].map((finding) => finding.code),
    recommendedLabels: recommendedLabelsForRoute(route, platforms),
    mutationAllowed: false,
    writesPerformed: false,
  };
}

function buildManagedComment(report) {
  const routeText = {
    candidate_for_agent: [
      'This report has enough bounded reproduction evidence for an automated investigation.',
      '该报告具备较明确的复现证据，可以进入自动分析候选队列。',
    ],
    needs_human: [
      'This report needs maintainer or platform-specific validation before implementation.',
      '该报告需要维护者或对应平台完成验证后再进入实现。',
    ],
    blocked_by_quality_gate: [
      'The existing issue quality gate still requires report corrections.',
      '现有 Issue 质量门禁仍要求补充报告信息。',
    ],
    observe_only: [
      'Automation did not find enough evidence to choose an implementation route.',
      '自动化尚未获得足够证据，不能确定实现路径。',
    ],
  }[report.route] || [
    'Automation left this report for maintainer review.',
    '自动化已将该报告保留给维护者复核。',
  ];

  return [
    MANAGED_COMMENT_MARKER,
    '## Maintenance automation / 维护自动化',
    '',
    ...routeText,
    '',
    '_This comment reports routing state only; it does not claim reproduction, a fix, or a release._',
    '_此评论只报告流转状态，不代表已经复现、修复或发布。_',
  ].join('\n');
}

function findManagedComment(comments, expectedLogin = null) {
  return (comments || []).find((comment) => {
    const login = comment.user?.login || '';
    const ownedIdentity = expectedLogin
      ? login === expectedLogin
      : comment.user?.type === 'Bot' || login.endsWith('[bot]');
    return ownedIdentity
      && String(comment.body || '').includes(MANAGED_COMMENT_MARKER);
  }) || null;
}

function decideCommentMutation(existingComment, nextBody) {
  if (!existingComment) return { action: 'create', body: nextBody };
  if (existingComment.body === nextBody) return { action: 'none' };
  return {
    action: 'update',
    commentId: existingComment.id,
    body: nextBody,
  };
}

function classifyChangedPaths(paths) {
  const result = {
    allowed: [],
    humanReview: [],
    protected: [],
  };
  for (const path of paths) {
    if (
      PROTECTED_PATHS.has(path)
      || PROTECTED_PATH_PREFIXES.some((prefix) => path.startsWith(prefix))
    ) {
      result.protected.push(path);
    } else if (HUMAN_REVIEW_PATH_PREFIXES.some((prefix) => path.startsWith(prefix))) {
      result.humanReview.push(path);
    } else {
      result.allowed.push(path);
    }
  }
  return result;
}

function scanTextForCredentials(text, sensitiveValues = []) {
  const findings = matchingSignalCodes(String(text || ''), CREDENTIAL_PATTERNS);
  for (const value of sensitiveValues) {
    if (typeof value === 'string' && value.length >= 8 && String(text || '').includes(value)) {
      findings.push('configured_secret_value');
      break;
    }
  }
  return [...new Set(findings)];
}

module.exports = {
  AUTOMATION_LABELS,
  MANAGED_COMMENT_MARKER,
  POLICY_VERSION,
  analyzeIssue,
  buildManagedComment,
  classifyChangedPaths,
  decideCommentMutation,
  findManagedComment,
  scanTextForCredentials,
};
