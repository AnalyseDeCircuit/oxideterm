'use strict';

const GATE_COMMENT_MARKER = '<!-- oxideterm-issue-quality:v1 -->';
const CORRECTION_NEEDED_LABEL = 'incomplete';
const QUALITY_BYPASS_LABEL = 'quality-check-exempt';
const TRUSTED_AUTHOR_ASSOCIATIONS = new Set(['OWNER', 'MEMBER', 'COLLABORATOR']);
// These repository roles can make a durable human triage decision for an issue.
const QUALITY_OVERRIDE_PERMISSIONS = new Set(['admin', 'maintain', 'write', 'triage']);

const REQUIRED_SECTIONS = [
  {
    label: 'bug',
    headings: [
      'OxideTerm version / 版本',
      'Platform / 平台',
      'Summary / 简述',
      'Steps to reproduce / 复现步骤',
      'Expected vs actual / 预期与实际',
    ],
  },
  {
    label: 'enhancement',
    headings: [
      'OxideTerm version / 版本',
      'Problem or use case / 问题或使用场景',
      'Proposed solution / 期望方案',
      'Why is this important? / 为什么这个功能对你重要？',
    ],
  },
  {
    label: 'compatibility',
    headings: [
      'OxideTerm version / 版本',
      'Client platform / 客户端平台',
      'Authentication method / 认证方式',
      'SSH server details / 服务端信息',
      'Error message or behavior / 错误信息或现象',
      'Working client comparison / 可正常连接的客户端对比',
    ],
  },
];

const NON_NARRATIVE_HEADING_PREFIXES = [
  'oxideterm version',
  'platform',
  'client platform',
  'area',
  'authentication method',
  'checklist',
];

function normalizeHeading(value) {
  return value.trim().replace(/\s+/g, ' ').toLocaleLowerCase('en-US');
}

function parseSections(markdown) {
  const headers = [...markdown.matchAll(/^###\s+(.+?)\s*$/gm)];
  const sections = new Map();
  for (let index = 0; index < headers.length; index += 1) {
    const current = headers[index];
    const contentStart = current.index + current[0].length;
    const contentEnd = headers[index + 1]?.index ?? markdown.length;
    sections.set(
      normalizeHeading(current[1]),
      markdown.slice(contentStart, contentEnd).trim()
    );
  }
  return sections;
}

function normalizeAnswer(value) {
  return value
    .trim()
    .replace(/^_+|_+$/g, '')
    .replace(/\s+/g, ' ')
    .toLocaleLowerCase('en-US');
}

function isMissingAnswer(value) {
  if (!value || !value.trim()) return true;
  const answer = normalizeAnswer(value);
  return /^(?:n\/?a|none|null|no response|nope|无|没有|不知道|你们?自己想|你看着办|you figure it out|you decide)$/.test(answer);
}

function meaningfulCharacterCount(value) {
  return Array.from(value.replace(/[\p{P}\p{S}\s]/gu, '')).length;
}

function plainBodyText(markdown) {
  return markdown
    .replace(/^###\s+.*$/gm, '')
    .replace(/^\s*-\s*\[[ xX]\].*$/gm, '')
    .replace(/[>*_`#~-]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function narrativeAnswers(sections) {
  return [...sections.entries()]
    .filter(([heading, answer]) => {
      const metadataOnly = NON_NARRATIVE_HEADING_PREFIXES.some((prefix) =>
        heading.startsWith(prefix)
      );
      return !metadataOnly && !isMissingAnswer(answer) && meaningfulCharacterCount(answer) >= 8;
    })
    .map(([, answer]) => normalizeAnswer(answer).replace(/\s+/g, ''));
}

function hasRepeatedNarrative(sections) {
  const answers = narrativeAnswers(sections);
  if (answers.length < 3) return false;
  const frequency = new Map();
  for (const answer of answers) {
    frequency.set(answer, (frequency.get(answer) || 0) + 1);
  }
  return Math.max(...frequency.values()) >= 3;
}

function findRequiredSectionPolicy(labels) {
  const labelSet = new Set(labels);
  return REQUIRED_SECTIONS.find((policy) => labelSet.has(policy.label));
}

function readSubmittedVersion(body) {
  const sections = parseSections(body);
  const value = sections.get(normalizeHeading('OxideTerm version / 版本'));
  if (!value) return null;
  const match = value.match(/(?:^|\s)v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z][0-9A-Za-z.-]*)?)(?:\s|$)/);
  return match?.[1] || null;
}

function normalizeReleaseName(tagName) {
  return tagName
    .replace(/^(?:(?:gpui|native|rustnative)-)?v/, '')
    .trim();
}

function hasUsefulArtifact(body) {
  return /```|!\[[^\]]*\]\(|<img\b|\b(?:error|log|stacktrace|panic|crash|failed|exception)\b/i.test(body);
}

function containsHostileLanguage(title, body) {
  const combined = `${title}\n${body}`;
  const patterns = [
    /\b(?:incompetent|stupid|idiot|moron|dumb[- ]?ass|brain[- ]?dead)\b/i,
    /废物|傻[逼比叉缺]|脑[残瘫]|智[障商低]|弱智|狗屎|你[妈马]|操你|脑子有/,
    /这也叫|就这[？?]|糊弄|骗人/,
  ];
  return patterns.some((pattern) => pattern.test(combined));
}

function evaluateIssue({ title, body, labels, releasedVersions = [] }) {
  const sections = parseSections(body);
  const blockingFindings = [];
  const reviewFindings = [];

  if (meaningfulCharacterCount(title) < 4) {
    blockingFindings.push({ code: 'title_needs_detail' });
  }

  const sectionPolicy = findRequiredSectionPolicy(labels);
  if (sectionPolicy) {
    for (const heading of sectionPolicy.headings) {
      const answer = sections.get(normalizeHeading(heading));
      if (isMissingAnswer(answer)) {
        blockingFindings.push({ code: 'required_section_missing', heading });
      }
    }
  } else if (meaningfulCharacterCount(plainBodyText(body)) < 12) {
    blockingFindings.push({ code: 'description_needs_detail' });
  }

  if (hasRepeatedNarrative(sections)) {
    blockingFindings.push({ code: 'repeated_section_content' });
  }

  if (labels.includes('bug')) {
    const reproduction = sections.get(normalizeHeading('Steps to reproduce / 复现步骤')) || '';
    if (meaningfulCharacterCount(reproduction) < 12 && !hasUsefulArtifact(body)) {
      reviewFindings.push({ code: 'reproduction_evidence_thin' });
    }
  }

  const submittedVersion = readSubmittedVersion(body);
  if (
    submittedVersion
    && releasedVersions.length > 0
    && !releasedVersions.includes(submittedVersion)
  ) {
    blockingFindings.push({
      code: 'release_version_unverified',
      version: submittedVersion,
    });
  }

  if (containsHostileLanguage(title, body)) {
    reviewFindings.push({ code: 'communication_needs_review' });
  }

  return { blockingFindings, reviewFindings };
}

function blockingFindingText(finding) {
  switch (finding.code) {
    case 'title_needs_detail':
      return '- The title needs a clearer summary. / 标题需要更明确地概括问题。';
    case 'description_needs_detail':
      return '- The description needs more concrete information. / 描述需要补充具体信息。';
    case 'required_section_missing':
      return `- Complete the required section: **${finding.heading}**. / 请完整填写必填部分：**${finding.heading}**。`;
    case 'repeated_section_content':
      return '- Different template sections need distinct answers. / 模板中的不同部分需要分别填写。';
    case 'release_version_unverified':
      return `- The reported version **${finding.version}** does not match any released version. Please fill in the exact version from Settings → Help & About. / 所填版本 **${finding.version}** 不存在于任何已发布版本中，请填写 设置 → 帮助与关于 中的准确版本号。`;
    default:
      return '- Additional report details are required. / 需要补充报告信息。';
  }
}

function buildCorrectionNotice(findings) {
  return [
    GATE_COMMENT_MARKER,
    '## Report needs correction / 报告需要补充',
    '',
    'This issue was closed because required report information is missing.',
    '此 Issue 因缺少必要信息而被暂时关闭。',
    '',
    ...findings.map(blockingFindingText),
    '',
    'Edit the title or body in this issue. The quality check will run again automatically and reopen the issue when the blocking items are resolved.',
    '请直接编辑当前 Issue 的标题或正文。修改完成后会自动重新检查；阻断项解决后，Issue 将自动重新打开。',
  ].join('\n');
}

function buildRecoveryNotice(reviewFindings) {
  const reviewPending = reviewFindings.length > 0;
  return [
    GATE_COMMENT_MARKER,
    '## Report accepted / 报告已通过检查',
    '',
    'The blocking format issues were resolved, so this issue has been reopened automatically.',
    '阻断性的格式问题已经解决，此 Issue 已自动重新打开。',
    ...(reviewPending
      ? [
        '',
        'A non-blocking review label remains for maintainer triage.',
        '仍有非阻断性的人工复核标签，维护者会继续确认。',
      ]
      : []),
  ].join('\n');
}

function buildBypassNotice() {
  return [
    GATE_COMMENT_MARKER,
    '## Automated check bypassed / 已跳过自动检查',
    '',
    'A maintainer exempted this issue from automated quality enforcement.',
    '维护者已将此 Issue 设为不受自动质量门禁限制。',
  ].join('\n');
}

function labelsForReviewFindings(findings) {
  const labels = new Set();
  for (const finding of findings) {
    if (finding.code === 'reproduction_evidence_thin') {
      labels.add('needs-reproduction-steps');
    } else if (finding.code === 'communication_needs_review') {
      labels.add('toxic-communication');
    }
  }
  return [...labels];
}

function decideStateChange({
  currentState,
  hasBlockingFindings,
  correctionLabelPresent,
  gateCommentPresent,
}) {
  if (hasBlockingFindings) {
    return currentState === 'closed' ? 'keep' : 'close';
  }
  const gateOwnsCorrection = correctionLabelPresent && gateCommentPresent;
  if (currentState === 'closed' && gateOwnsCorrection) {
    return 'reopen';
  }
  return 'keep';
}

// A maintainer reopening an issue explicitly takes ownership away from the format gate.
function isTrustedManualReopen({ action, senderType, actorPermission }) {
  return action === 'reopened'
    && senderType === 'User'
    && QUALITY_OVERRIDE_PERMISSIONS.has(actorPermission);
}

module.exports = {
  CORRECTION_NEEDED_LABEL,
  GATE_COMMENT_MARKER,
  QUALITY_BYPASS_LABEL,
  TRUSTED_AUTHOR_ASSOCIATIONS,
  buildBypassNotice,
  buildCorrectionNotice,
  buildRecoveryNotice,
  decideStateChange,
  evaluateIssue,
  isTrustedManualReopen,
  labelsForReviewFindings,
  normalizeReleaseName,
  parseSections,
  readSubmittedVersion,
};
