# OxideTerm Legal Notice

Last updated: 2026-07-03

This document is provided in 11 languages:

- [English](#english)
- [简体中文](#简体中文)
- [繁體中文](#繁體中文)
- [日本語](#日本語)
- [한국어](#한국어)
- [Français](#français)
- [Deutsch](#deutsch)
- [Español](#español)
- [Italiano](#italiano)
- [Português do Brasil](#português-do-brasil)
- [Tiếng Việt](#tiếng-việt)

If translations differ, the English section is the reference version.

---

## English

### 1. Purpose

OxideTerm is a local-first desktop operations workspace for authorized system administration, debugging, remote access, file transfer, serial access, remote desktop, network diagnostics, and AI-assisted workflows. This notice explains acceptable use, privacy boundaries, third-party responsibilities, and warranty limitations.

This notice is not legal advice. If you need a legal opinion for a specific jurisdiction, commercial deployment, regulated environment, export scenario, or customer contract, consult a qualified lawyer.

### 2. License and Warranty

OxideTerm is distributed under the GNU General Public License version 3.0 only (GPL-3.0-only). The full license text is available in the repository `LICENSE` file. If this notice conflicts with the GPL license text, the GPL license text controls software licensing.

OxideTerm is provided without warranty to the maximum extent permitted by applicable law. No guarantee is made regarding availability, correctness, security, fitness for a particular purpose, compliance suitability, or uninterrupted operation.

### 3. Authorized Use

Use OxideTerm only on systems, devices, networks, accounts, files, and services that you own, manage, or have explicit permission to operate. This includes SSH, SFTP, Telnet, Serial, RDP, VNC, Raw TCP, Raw UDP, port forwarding, proxy jump, file editing, cloud sync, portable bundles, plugins, and AI-assisted actions.

Security testing, incident response, research, red-team work, and troubleshooting must stay within a clearly authorized scope.

### 4. Prohibited Use

Do not use OxideTerm for unauthorized access, credential attacks, stealthy remote control, malware deployment, vulnerability exploitation outside authorization, evasion of security controls, bypassing network restrictions, destructive activity, data exfiltration, or abuse of third-party infrastructure.

Do not market or represent OxideTerm as a certified security, cryptographic, government, compliance, or regulated-industry product unless you have independently obtained the required approvals.

### 5. Privacy and Local Data

OxideTerm is designed as a local desktop client. By default, it does not require an OxideTerm account, collect telemetry, or upload terminal content, connection profiles, command history, private keys, credentials, local files, settings, logs, or diagnostics to an OxideTerm-operated service.

User-configured features may send data to endpoints selected by the user, including remote hosts, cloud sync backends, AI model providers, plugin targets, update servers, proxy servers, or storage services. Review destinations and data scope before enabling those features.

### 6. Secrets and Shared Materials

Credentials and API keys should remain in platform-secure storage where supported. Before sharing logs, screenshots, exported bundles, diagnostics, terminal transcripts, AI prompts, or bug reports, review and redact passwords, private keys, tokens, API keys, cookies, usernames, hostnames, IP addresses, internal paths, and organization-specific identifiers.

### 7. AI Features

OxideSens may use user-configured model providers, local models, OpenAI-compatible endpoints, retrieval indexes, tool policies, and selected workspace context. AI output can be wrong, incomplete, or unsafe. Review suggestions, commands, file edits, and tool actions before execution.

Do not send private keys, passwords, production secrets, confidential data, regulated data, or other sensitive information to model providers unless you are authorized to do so and accept that provider's terms and data practices.

### 8. Plugins, Third Parties, and Network Tools

Only install plugins from sources you trust. Plugins may request access to terminal content, files, network resources, workspace state, settings, or host APIs depending on their declared capabilities.

Port forwarding, proxy jump, upstream proxy, Raw TCP, and Raw UDP features are provided for authorized administration and debugging. You are responsible for ensuring that routing, forwarding, proxying, and traffic behavior comply with your authorization boundary, organizational policy, service terms, and applicable law.

Third-party services, model providers, cloud storage, remote hosts, operating-system services, package registries, and plugins are governed by their own terms, licenses, privacy policies, security controls, availability, and data-handling practices.

### 9. User Responsibility

You are responsible for reviewing operations before execution, protecting secrets, maintaining backups, testing critical workflows, complying with applicable requirements, and using OxideTerm only within authorized boundaries.

---

## 简体中文

### 1. 目的

OxideTerm 是本地优先的桌面运维工作区，用于已授权的系统管理、调试、远程访问、文件传输、串口访问、远程桌面、网络诊断和 AI 辅助工作流。本文说明可接受使用、隐私边界、第三方责任和无担保限制。

本文不是法律意见。如果你需要针对特定司法辖区、商业部署、受监管环境、出口场景或客户合同的法律判断，请咨询合格律师。

### 2. 许可证与担保

OxideTerm 以 GNU 通用公共许可证第 3 版（GPL-3.0-only）发布。完整许可证文本位于仓库根目录的 `LICENSE` 文件。若本文与 GPL 许可证文本存在冲突，以 GPL 许可证文本对软件授权的规定为准。

在适用法律允许的最大范围内，OxideTerm 不提供任何担保，包括但不限于可用性、正确性、安全性、特定用途适用性、合规适用性或持续运行。

### 3. 授权使用

请只在你拥有、管理或明确获准操作的系统、设备、网络、账号、文件和服务上使用 OxideTerm。这包括 SSH、SFTP、Telnet、串口、RDP、VNC、Raw TCP、Raw UDP、端口转发、跳板机、文件编辑、云同步、便携包、插件和 AI 辅助操作。

安全测试、应急响应、研究、红队和故障排查必须保持在明确授权范围内。

### 4. 禁止用途

请勿将 OxideTerm 用于未授权访问、凭据攻击、隐蔽远控、恶意软件部署、超出授权的漏洞利用、规避安全控制、绕过网络限制、破坏性活动、数据外泄或滥用第三方基础设施。

除非你已经独立取得所需资质，否则不要将 OxideTerm 宣称为经过认证的安全、密码、政府、合规或受监管行业产品。

### 5. 隐私与本地数据

OxideTerm 设计为本地桌面客户端。默认情况下，它不要求 OxideTerm 账号，不收集遥测，也不会把终端内容、连接配置、命令历史、私钥、凭据、本地文件、设置、日志或诊断信息上传到 OxideTerm 运营的服务。

用户配置的功能可能会把数据发送到用户选择的端点，例如远程主机、云同步后端、AI 模型供应商、插件目标、更新服务器、代理服务器或存储服务。启用前请检查目标和数据范围。

### 6. 秘密与分享材料

凭据和 API Key 应尽量保存在平台安全存储中。分享日志、截图、导出包、诊断信息、终端记录、AI 提示词或问题报告前，请先检查并脱敏密码、私钥、令牌、API Key、Cookie、用户名、主机名、IP 地址、内部路径和组织内部标识。

### 7. AI 功能

OxideSens 可能使用用户配置的模型供应商、本地模型、OpenAI 兼容端点、检索索引、工具策略和用户选择的工作区上下文。AI 输出可能错误、不完整或不安全。执行前请审查建议、命令、文件编辑和工具动作。

除非你获得授权并接受供应商条款和数据处理方式，否则不要把私钥、密码、生产秘密、机密数据、受监管数据或其他敏感信息发送给模型供应商。

### 8. 插件、第三方与网络工具

只安装来自可信来源的插件。根据声明的能力，插件可能请求访问终端内容、文件、网络资源、工作区状态、设置或主机接口。

端口转发、跳板机、上游代理、Raw TCP 和 Raw UDP 功能用于已授权的管理和调试。用户需要自行确认路由、转发、代理和流量行为符合授权边界、组织政策、服务条款和适用法律。

第三方服务、模型供应商、云存储、远程主机、操作系统服务、软件包注册表和插件受各自条款、许可证、隐私政策、安全控制、可用性和数据处理方式约束。

### 9. 用户责任

用户需要自行负责在执行前审查操作、保护秘密、维护备份、测试关键工作流、遵守适用要求，并只在授权边界内使用 OxideTerm。

---

## 繁體中文

### 1. 目的

OxideTerm 是本機優先的桌面維運工作區，用於已授權的系統管理、除錯、遠端存取、檔案傳輸、串口存取、遠端桌面、網路診斷和 AI 輔助工作流。本文說明可接受使用、隱私邊界、第三方責任和無擔保限制。

本文不是法律意見。如果你需要針對特定司法轄區、商業部署、受監管環境、出口場景或客戶合約的法律判斷，請諮詢合格律師。

### 2. 授權條款與擔保

OxideTerm 以 GNU 通用公共授權條款第 3 版（GPL-3.0-only）發布。完整授權條款位於倉庫根目錄的 `LICENSE` 檔案。若本文與 GPL 授權條款文字存在衝突，以 GPL 授權條款文字對軟體授權的規定為準。

在適用法律允許的最大範圍內，OxideTerm 不提供任何擔保，包括但不限於可用性、正確性、安全性、特定用途適用性、合規適用性或持續運行。

### 3. 授權使用

請只在你擁有、管理或明確獲准操作的系統、裝置、網路、帳號、檔案和服務上使用 OxideTerm。這包括 SSH、SFTP、Telnet、串口、RDP、VNC、Raw TCP、Raw UDP、連接埠轉送、跳板機、檔案編輯、雲端同步、便攜包、插件和 AI 輔助操作。

安全測試、應急回應、研究、紅隊和故障排查必須保持在明確授權範圍內。

### 4. 禁止用途

請勿將 OxideTerm 用於未授權存取、憑證攻擊、隱蔽遠控、惡意軟體部署、超出授權的漏洞利用、規避安全控制、繞過網路限制、破壞性活動、資料外洩或濫用第三方基礎設施。

除非你已經獨立取得所需資質，否則不要將 OxideTerm 宣稱為經過認證的安全、密碼、政府、合規或受監管行業產品。

### 5. 隱私與本機資料

OxideTerm 設計為本機桌面客戶端。預設情況下，它不要求 OxideTerm 帳號，不收集遙測，也不會把終端機內容、連線設定、命令歷史、私鑰、憑證、本機檔案、設定、日誌或診斷資訊上傳到 OxideTerm 營運的服務。

使用者設定的功能可能會把資料傳送到使用者選擇的端點，例如遠端主機、雲端同步後端、AI 模型供應商、插件目標、更新伺服器、代理伺服器或儲存服務。啟用前請檢查目標和資料範圍。

### 6. 秘密與分享材料

憑證和 API Key 應盡量保存在平台安全儲存中。分享日誌、截圖、匯出包、診斷資訊、終端機記錄、AI 提示詞或問題報告前，請先檢查並遮蔽密碼、私鑰、令牌、API Key、Cookie、使用者名稱、主機名、IP 位址、內部路徑和組織內部標識。

### 7. AI 功能

OxideSens 可能使用使用者設定的模型供應商、本機模型、OpenAI 相容端點、檢索索引、工具策略和使用者選取的工作區內容。AI 輸出可能錯誤、不完整或不安全。執行前請審查建議、命令、檔案編輯和工具動作。

除非你獲得授權並接受供應商條款和資料處理方式，否則不要把私鑰、密碼、生產秘密、機密資料、受監管資料或其他敏感資訊傳送給模型供應商。

### 8. 插件、第三方與網路工具

只安裝來自可信來源的插件。根據聲明的能力，插件可能請求存取終端機內容、檔案、網路資源、工作區狀態、設定或主機接口。

連接埠轉送、跳板機、上游代理、Raw TCP 和 Raw UDP 功能用於已授權的管理和除錯。使用者需要自行確認路由、轉送、代理和流量行為符合授權邊界、組織政策、服務條款和適用法律。

第三方服務、模型供應商、雲端儲存、遠端主機、作業系統服務、套件註冊表和插件受各自條款、授權條款、隱私政策、安全控制、可用性和資料處理方式約束。

### 9. 使用者責任

使用者需要自行負責在執行前審查操作、保護秘密、維護備份、測試關鍵工作流、遵守適用要求，並只在授權邊界內使用 OxideTerm。

---

## 日本語

### 1. 目的

OxideTerm は、許可されたシステム管理、デバッグ、リモートアクセス、ファイル転送、シリアルアクセス、リモートデスクトップ、ネットワーク診断、AI 支援ワークフローのためのローカル優先デスクトップ運用ワークスペースです。本通知は、許容される利用、プライバシー境界、第三者に関する責任、保証の制限を説明します。

本通知は法律助言ではありません。特定の法域、商用展開、規制環境、輸出場面、顧客契約について法的判断が必要な場合は、資格のある法律専門家に相談してください。

### 2. ライセンスと保証

OxideTerm は GNU General Public License version 3.0 only（GPL-3.0-only）で配布されます。完全なライセンス文はリポジトリの `LICENSE` ファイルにあります。本通知と GPL ライセンス文が矛盾する場合、ソフトウェアのライセンスについては GPL ライセンス文が優先します。

適用法で認められる最大限の範囲で、OxideTerm はいかなる保証もなく提供されます。可用性、正確性、安全性、特定目的への適合性、コンプライアンス適合性、継続的な動作について保証しません。

### 3. 許可された利用

OxideTerm は、所有、管理、または明示的に操作を許可されたシステム、デバイス、ネットワーク、アカウント、ファイル、サービスでのみ使用してください。これには SSH、SFTP、Telnet、Serial、RDP、VNC、Raw TCP、Raw UDP、ポート転送、ProxyJump、ファイル編集、クラウド同期、ポータブルバンドル、プラグイン、AI 支援操作が含まれます。

セキュリティテスト、インシデント対応、研究、レッドチーム、トラブルシューティングは、明確に許可された範囲内で行う必要があります。

### 4. 禁止される利用

OxideTerm を不正アクセス、認証情報への攻撃、隠れたリモート操作、マルウェア展開、許可範囲外の脆弱性悪用、セキュリティ制御の回避、ネットワーク制限の回避、破壊的行為、データ流出、第三者インフラの悪用に使用しないでください。

必要な認証を独自に取得していない限り、OxideTerm を認証済みのセキュリティ、暗号、政府、コンプライアンス、規制産業向け製品として表示しないでください。

### 5. プライバシーとローカルデータ

OxideTerm はローカルデスクトップクライアントとして設計されています。既定では OxideTerm アカウントを要求せず、テレメトリを収集せず、ターミナル内容、接続プロファイル、コマンド履歴、秘密鍵、認証情報、ローカルファイル、設定、ログ、診断情報を OxideTerm が運営するサービスへアップロードしません。

ユーザーが設定した機能は、ユーザーが選択したエンドポイントへデータを送信することがあります。例として、リモートホスト、クラウド同期バックエンド、AI モデルプロバイダー、プラグインターゲット、更新サーバー、プロキシサーバー、ストレージサービスがあります。これらを有効にする前に宛先とデータ範囲を確認してください。

### 6. 秘密情報と共有資料

認証情報と API キーは、対応している場合はプラットフォームの安全なストレージに保持してください。ログ、スクリーンショット、エクスポート、診断情報、ターミナル記録、AI プロンプト、バグ報告を共有する前に、パスワード、秘密鍵、トークン、API キー、Cookie、ユーザー名、ホスト名、IP アドレス、内部パス、組織固有の識別子を確認して伏せてください。

### 7. AI 機能

OxideSens は、ユーザーが設定したモデルプロバイダー、ローカルモデル、OpenAI 互換エンドポイント、検索インデックス、ツールポリシー、選択されたワークスペースコンテキストを使用する場合があります。AI 出力は誤り、不完全、または安全でない可能性があります。実行前に提案、コマンド、ファイル編集、ツール操作を確認してください。

許可されており、プロバイダーの規約とデータ取り扱いを受け入れる場合を除き、秘密鍵、パスワード、本番環境の秘密情報、機密データ、規制対象データ、その他の敏感情報をモデルプロバイダーへ送信しないでください。

### 8. プラグイン、第三者、ネットワークツール

信頼できる提供元のプラグインのみをインストールしてください。宣言された機能に応じて、プラグインはターミナル内容、ファイル、ネットワークリソース、ワークスペース状態、設定、ホスト API へのアクセスを要求する場合があります。

ポート転送、ProxyJump、上流プロキシ、Raw TCP、Raw UDP は、許可された管理とデバッグのために提供されます。ルーティング、転送、プロキシ、トラフィック動作が、許可範囲、組織ポリシー、サービス規約、適用法に従っていることを確認する責任はユーザーにあります。

第三者サービス、モデルプロバイダー、クラウドストレージ、リモートホスト、OS サービス、パッケージレジストリ、プラグインには、それぞれの規約、ライセンス、プライバシーポリシー、セキュリティ制御、可用性、データ取り扱いが適用されます。

### 9. ユーザーの責任

操作前の確認、秘密情報の保護、バックアップの維持、重要なワークフローのテスト、適用される要件の遵守、許可範囲内での OxideTerm 利用は、ユーザーの責任です。

---

## 한국어

### 1. 목적

OxideTerm은 승인된 시스템 관리, 디버깅, 원격 접근, 파일 전송, 시리얼 접근, 원격 데스크톱, 네트워크 진단 및 AI 지원 워크플로를 위한 로컬 우선 데스크톱 운영 작업 공간입니다. 이 고지는 허용되는 사용, 개인정보 경계, 제3자 책임 및 보증 제한을 설명합니다.

이 고지는 법률 자문이 아닙니다. 특정 관할권, 상업적 배포, 규제 환경, 수출 상황 또는 고객 계약에 대한 법적 의견이 필요하면 자격 있는 법률 전문가와 상담하세요.

### 2. 라이선스와 보증

OxideTerm은 GNU General Public License version 3.0 only(GPL-3.0-only)에 따라 배포됩니다. 전체 라이선스 문서는 저장소의 `LICENSE` 파일에 있습니다. 이 고지와 GPL 라이선스 문구가 충돌하는 경우, 소프트웨어 라이선스에는 GPL 라이선스 문구가 우선합니다.

적용 법률이 허용하는 최대 범위에서 OxideTerm은 어떠한 보증 없이 제공됩니다. 가용성, 정확성, 보안성, 특정 목적 적합성, 규정 준수 적합성 또는 중단 없는 동작을 보장하지 않습니다.

### 3. 승인된 사용

OxideTerm은 소유하거나 관리하거나 명시적으로 운영 권한을 받은 시스템, 장치, 네트워크, 계정, 파일 및 서비스에서만 사용하세요. 여기에는 SSH, SFTP, Telnet, Serial, RDP, VNC, Raw TCP, Raw UDP, 포트 포워딩, ProxyJump, 파일 편집, 클라우드 동기화, 휴대용 번들, 플러그인 및 AI 지원 작업이 포함됩니다.

보안 테스트, 사고 대응, 연구, 레드팀 활동 및 문제 해결은 명확히 승인된 범위 안에서만 수행해야 합니다.

### 4. 금지된 사용

OxideTerm을 무단 접근, 자격 증명 공격, 은닉 원격 제어, 악성코드 배포, 승인 범위를 벗어난 취약점 악용, 보안 통제 우회, 네트워크 제한 우회, 파괴적 활동, 데이터 유출 또는 제3자 인프라 남용에 사용하지 마세요.

필요한 승인을 독립적으로 취득하지 않은 한, OxideTerm을 인증된 보안, 암호화, 정부, 규정 준수 또는 규제 산업용 제품으로 표시하지 마세요.

### 5. 개인정보와 로컬 데이터

OxideTerm은 로컬 데스크톱 클라이언트로 설계되었습니다. 기본적으로 OxideTerm 계정을 요구하지 않고, 텔레메트리를 수집하지 않으며, 터미널 내용, 연결 프로필, 명령 기록, 개인 키, 자격 증명, 로컬 파일, 설정, 로그 또는 진단 정보를 OxideTerm 운영 서비스로 업로드하지 않습니다.

사용자가 구성한 기능은 사용자가 선택한 엔드포인트로 데이터를 보낼 수 있습니다. 예로는 원격 호스트, 클라우드 동기화 백엔드, AI 모델 제공자, 플러그인 대상, 업데이트 서버, 프록시 서버 또는 저장소 서비스가 있습니다. 활성화하기 전에 대상과 데이터 범위를 검토하세요.

### 6. 비밀 정보와 공유 자료

자격 증명과 API 키는 지원되는 경우 플랫폼 보안 저장소에 보관해야 합니다. 로그, 스크린샷, 내보낸 번들, 진단 정보, 터미널 기록, AI 프롬프트 또는 버그 보고서를 공유하기 전에 비밀번호, 개인 키, 토큰, API 키, 쿠키, 사용자 이름, 호스트 이름, IP 주소, 내부 경로 및 조직 식별자를 검토하고 가리세요.

### 7. AI 기능

OxideSens는 사용자가 구성한 모델 제공자, 로컬 모델, OpenAI 호환 엔드포인트, 검색 인덱스, 도구 정책 및 선택된 작업 공간 컨텍스트를 사용할 수 있습니다. AI 출력은 틀리거나 불완전하거나 안전하지 않을 수 있습니다. 실행 전에 제안, 명령, 파일 편집 및 도구 작업을 검토하세요.

승인을 받았고 제공자의 약관과 데이터 처리 방식을 수락한 경우가 아니라면, 개인 키, 비밀번호, 운영 환경 비밀값, 기밀 데이터, 규제 대상 데이터 또는 기타 민감한 정보를 모델 제공자에게 보내지 마세요.

### 8. 플러그인, 제3자 및 네트워크 도구

신뢰할 수 있는 출처의 플러그인만 설치하세요. 선언된 기능에 따라 플러그인은 터미널 내용, 파일, 네트워크 리소스, 작업 공간 상태, 설정 또는 호스트 API 접근을 요청할 수 있습니다.

포트 포워딩, ProxyJump, 업스트림 프록시, Raw TCP 및 Raw UDP 기능은 승인된 관리와 디버깅을 위해 제공됩니다. 라우팅, 전달, 프록시 및 트래픽 동작이 승인 범위, 조직 정책, 서비스 약관 및 적용 법률을 준수하는지 확인할 책임은 사용자에게 있습니다.

제3자 서비스, 모델 제공자, 클라우드 저장소, 원격 호스트, 운영체제 서비스, 패키지 레지스트리 및 플러그인은 각각의 약관, 라이선스, 개인정보 처리방침, 보안 통제, 가용성 및 데이터 처리 방식의 적용을 받습니다.

### 9. 사용자 책임

작업 실행 전 검토, 비밀 정보 보호, 백업 유지, 중요 워크플로 테스트, 적용 요건 준수 및 승인 범위 내 OxideTerm 사용은 사용자 책임입니다.

---

## Français

### 1. Objet

OxideTerm est un espace de travail d'exploitation local-first pour l'administration système, le débogage, l'accès distant, le transfert de fichiers, l'accès série, le bureau à distance, le diagnostic réseau et les flux assistés par IA autorisés. Cette notice explique l'utilisation acceptable, les limites de confidentialité, les responsabilités liées aux tiers et les limites de garantie.

Cette notice n'est pas un avis juridique. Si vous avez besoin d'un avis pour une juridiction, un déploiement commercial, un environnement réglementé, un scénario d'exportation ou un contrat client, consultez un juriste qualifié.

### 2. Licence et garantie

OxideTerm est distribué sous GNU General Public License version 3.0 only (GPL-3.0-only). Le texte complet de la licence se trouve dans le fichier `LICENSE` du dépôt. En cas de conflit entre cette notice et le texte GPL, le texte GPL prévaut pour la licence du logiciel.

OxideTerm est fourni sans garantie dans toute la mesure permise par la loi applicable. Aucune garantie n'est donnée quant à la disponibilité, l'exactitude, la sécurité, l'adéquation à un usage particulier, l'aptitude à la conformité ou le fonctionnement ininterrompu.

### 3. Utilisation autorisée

Utilisez OxideTerm uniquement sur les systèmes, appareils, réseaux, comptes, fichiers et services que vous possédez, administrez ou êtes explicitement autorisé à exploiter. Cela inclut SSH, SFTP, Telnet, Serial, RDP, VNC, Raw TCP, Raw UDP, le transfert de ports, le proxy jump, l'édition de fichiers, la synchronisation cloud, les bundles portables, les plugins et les actions assistées par IA.

Les tests de sécurité, la réponse à incident, la recherche, le red team et le dépannage doivent rester dans un périmètre clairement autorisé.

### 4. Utilisation interdite

N'utilisez pas OxideTerm pour un accès non autorisé, des attaques sur identifiants, le contrôle distant dissimulé, le déploiement de logiciels malveillants, l'exploitation de vulnérabilités hors autorisation, le contournement de contrôles de sécurité, le contournement de restrictions réseau, des activités destructrices, l'exfiltration de données ou l'abus d'infrastructures tierces.

Ne présentez pas OxideTerm comme un produit certifié de sécurité, cryptographie, gouvernement, conformité ou secteur réglementé, sauf si vous avez obtenu indépendamment les approbations requises.

### 5. Confidentialité et données locales

OxideTerm est conçu comme un client de bureau local. Par défaut, il ne nécessite pas de compte OxideTerm, ne collecte pas de télémétrie et ne téléverse pas le contenu du terminal, les profils de connexion, l'historique des commandes, les clés privées, les identifiants, les fichiers locaux, les paramètres, les journaux ou les diagnostics vers un service exploité par OxideTerm.

Les fonctions configurées par l'utilisateur peuvent envoyer des données vers des points de terminaison choisis par l'utilisateur, notamment des hôtes distants, des backends de synchronisation cloud, des fournisseurs de modèles IA, des cibles de plugins, des serveurs de mise à jour, des serveurs proxy ou des services de stockage. Vérifiez les destinations et le périmètre des données avant de les activer.

### 6. Secrets et éléments partagés

Les identifiants et clés API doivent rester dans le stockage sécurisé de la plateforme lorsque celui-ci est disponible. Avant de partager journaux, captures d'écran, exports, diagnostics, transcriptions de terminal, prompts IA ou rapports de bug, vérifiez et masquez mots de passe, clés privées, jetons, clés API, cookies, noms d'utilisateur, noms d'hôte, adresses IP, chemins internes et identifiants propres à l'organisation.

### 7. Fonctions IA

OxideSens peut utiliser des fournisseurs de modèles configurés par l'utilisateur, des modèles locaux, des points de terminaison compatibles OpenAI, des index de recherche, des politiques d'outils et le contexte sélectionné de l'espace de travail. Les sorties IA peuvent être incorrectes, incomplètes ou dangereuses. Vérifiez les suggestions, commandes, modifications de fichiers et actions d'outils avant exécution.

N'envoyez pas de clés privées, mots de passe, secrets de production, données confidentielles, données réglementées ou autres informations sensibles à des fournisseurs de modèles, sauf si vous y êtes autorisé et acceptez leurs conditions et pratiques de traitement des données.

### 8. Plugins, tiers et outils réseau

Installez uniquement des plugins provenant de sources fiables. Selon leurs capacités déclarées, les plugins peuvent demander l'accès au contenu du terminal, aux fichiers, aux ressources réseau, à l'état de l'espace de travail, aux paramètres ou aux API hôte.

Le transfert de ports, le proxy jump, le proxy amont, Raw TCP et Raw UDP sont fournis pour l'administration et le débogage autorisés. Vous êtes responsable de vérifier que le routage, le transfert, le proxy et le trafic respectent votre périmètre d'autorisation, les politiques de votre organisation, les conditions de service et la loi applicable.

Les services tiers, fournisseurs de modèles, stockages cloud, hôtes distants, services du système d'exploitation, registres de paquets et plugins sont régis par leurs propres conditions, licences, politiques de confidentialité, contrôles de sécurité, disponibilité et pratiques de traitement des données.

### 9. Responsabilité de l'utilisateur

Vous êtes responsable de vérifier les opérations avant exécution, de protéger les secrets, de maintenir des sauvegardes, de tester les flux critiques, de respecter les exigences applicables et d'utiliser OxideTerm uniquement dans les limites autorisées.

---

## Deutsch

### 1. Zweck

OxideTerm ist ein lokal orientierter Desktop-Arbeitsbereich für autorisierte Systemadministration, Debugging, Fernzugriff, Dateiübertragung, seriellen Zugriff, Remote Desktop, Netzwerkdiagnose und KI-gestützte Arbeitsabläufe. Dieser Hinweis erklärt zulässige Nutzung, Datenschutzgrenzen, Verantwortlichkeiten gegenüber Dritten und Gewährleistungsbeschränkungen.

Dieser Hinweis ist keine Rechtsberatung. Wenn Sie eine rechtliche Einschätzung für eine bestimmte Rechtsordnung, kommerzielle Bereitstellung, regulierte Umgebung, ein Exportszenario oder einen Kundenvertrag benötigen, wenden Sie sich an qualifizierte Rechtsberatung.

### 2. Lizenz und Gewährleistung

OxideTerm wird unter der GNU General Public License version 3.0 only (GPL-3.0-only) verteilt. Der vollständige Lizenztext befindet sich in der Datei `LICENSE` des Repositorys. Wenn dieser Hinweis dem GPL-Lizenztext widerspricht, ist für die Softwarelizenzierung der GPL-Lizenztext maßgeblich.

OxideTerm wird im gesetzlich zulässigen Umfang ohne Gewährleistung bereitgestellt. Es wird keine Garantie für Verfügbarkeit, Richtigkeit, Sicherheit, Eignung für einen bestimmten Zweck, Compliance-Eignung oder unterbrechungsfreien Betrieb übernommen.

### 3. Autorisierte Nutzung

Verwenden Sie OxideTerm nur auf Systemen, Geräten, Netzwerken, Konten, Dateien und Diensten, die Sie besitzen, verwalten oder ausdrücklich bedienen dürfen. Dies umfasst SSH, SFTP, Telnet, Serial, RDP, VNC, Raw TCP, Raw UDP, Portweiterleitung, Proxy-Jump, Dateibearbeitung, Cloud-Sync, portable Bundles, Plugins und KI-gestützte Aktionen.

Sicherheitstests, Incident Response, Forschung, Red-Team-Arbeit und Fehlerbehebung müssen in einem klar autorisierten Umfang bleiben.

### 4. Verbotene Nutzung

Verwenden Sie OxideTerm nicht für unbefugten Zugriff, Angriffe auf Zugangsdaten, verdeckte Fernsteuerung, Malware-Bereitstellung, nicht autorisierte Schwachstellenausnutzung, Umgehung von Sicherheitskontrollen, Umgehung von Netzwerkbeschränkungen, destruktive Aktivitäten, Datenabfluss oder Missbrauch von Drittinfrastruktur.

Stellen Sie OxideTerm nicht als zertifiziertes Sicherheits-, Kryptografie-, Regierungs-, Compliance- oder reguliertes Branchenprodukt dar, sofern Sie die erforderlichen Genehmigungen nicht unabhängig erhalten haben.

### 5. Datenschutz und lokale Daten

OxideTerm ist als lokaler Desktop-Client konzipiert. Standardmäßig erfordert es kein OxideTerm-Konto, sammelt keine Telemetrie und lädt keine Terminalinhalte, Verbindungsprofile, Befehlsverläufe, privaten Schlüssel, Zugangsdaten, lokalen Dateien, Einstellungen, Logs oder Diagnosen zu einem von OxideTerm betriebenen Dienst hoch.

Vom Benutzer konfigurierte Funktionen können Daten an vom Benutzer ausgewählte Endpunkte senden, darunter Remote-Hosts, Cloud-Sync-Backends, KI-Modellanbieter, Plugin-Ziele, Update-Server, Proxy-Server oder Speicherdienste. Prüfen Sie Ziel und Datenumfang vor der Aktivierung.

### 6. Geheimnisse und geteilte Materialien

Zugangsdaten und API-Schlüssel sollten, sofern unterstützt, im sicheren Speicher der Plattform verbleiben. Prüfen und schwärzen Sie vor dem Teilen von Logs, Screenshots, Exporten, Diagnosen, Terminalmitschnitten, KI-Prompts oder Fehlerberichten Passwörter, private Schlüssel, Token, API-Schlüssel, Cookies, Benutzernamen, Hostnamen, IP-Adressen, interne Pfade und organisationsspezifische Kennungen.

### 7. KI-Funktionen

OxideSens kann vom Benutzer konfigurierte Modellanbieter, lokale Modelle, OpenAI-kompatible Endpunkte, Suchindizes, Tool-Richtlinien und ausgewählten Workspace-Kontext verwenden. KI-Ausgaben können falsch, unvollständig oder unsicher sein. Prüfen Sie Vorschläge, Befehle, Dateiänderungen und Tool-Aktionen vor der Ausführung.

Senden Sie keine privaten Schlüssel, Passwörter, Produktionsgeheimnisse, vertraulichen Daten, regulierten Daten oder andere sensible Informationen an Modellanbieter, sofern Sie dazu nicht autorisiert sind und deren Bedingungen sowie Datenverarbeitung akzeptieren.

### 8. Plugins, Dritte und Netzwerktools

Installieren Sie nur Plugins aus vertrauenswürdigen Quellen. Je nach deklarierten Fähigkeiten können Plugins Zugriff auf Terminalinhalte, Dateien, Netzwerkressourcen, Workspace-Status, Einstellungen oder Host-APIs anfordern.

Portweiterleitung, Proxy-Jump, Upstream-Proxy, Raw TCP und Raw UDP werden für autorisierte Administration und Debugging bereitgestellt. Sie sind dafür verantwortlich, dass Routing, Weiterleitung, Proxying und Datenverkehr innerhalb Ihrer Autorisierung, Organisationsrichtlinien, Dienstbedingungen und geltenden Gesetze bleiben.

Drittanbieter-Dienste, Modellanbieter, Cloud-Speicher, Remote-Hosts, Betriebssystemdienste, Paketregistries und Plugins unterliegen ihren eigenen Bedingungen, Lizenzen, Datenschutzrichtlinien, Sicherheitskontrollen, Verfügbarkeiten und Datenverarbeitungspraktiken.

### 9. Verantwortung des Benutzers

Sie sind verantwortlich für die Prüfung von Vorgängen vor der Ausführung, den Schutz von Geheimnissen, Backups, Tests kritischer Arbeitsabläufe, Einhaltung geltender Anforderungen und die Nutzung von OxideTerm nur innerhalb autorisierter Grenzen.

---

## Español

### 1. Propósito

OxideTerm es un espacio de trabajo de operaciones de escritorio local-first para administración de sistemas, depuración, acceso remoto, transferencia de archivos, acceso serial, escritorio remoto, diagnóstico de red y flujos asistidos por IA autorizados. Este aviso explica el uso aceptable, los límites de privacidad, las responsabilidades con terceros y las limitaciones de garantía.

Este aviso no es asesoramiento legal. Si necesitas una opinión legal para una jurisdicción, despliegue comercial, entorno regulado, escenario de exportación o contrato de cliente, consulta a un abogado cualificado.

### 2. Licencia y garantía

OxideTerm se distribuye bajo GNU General Public License version 3.0 only (GPL-3.0-only). El texto completo de la licencia está en el archivo `LICENSE` del repositorio. Si este aviso entra en conflicto con el texto de la GPL, el texto de la GPL controla la licencia del software.

OxideTerm se proporciona sin garantía en la máxima medida permitida por la ley aplicable. No se garantiza disponibilidad, corrección, seguridad, idoneidad para un propósito concreto, aptitud de cumplimiento ni funcionamiento ininterrumpido.

### 3. Uso autorizado

Usa OxideTerm solo en sistemas, dispositivos, redes, cuentas, archivos y servicios que posees, administras o tienes permiso explícito para operar. Esto incluye SSH, SFTP, Telnet, Serial, RDP, VNC, Raw TCP, Raw UDP, reenvío de puertos, proxy jump, edición de archivos, sincronización en la nube, paquetes portables, plugins y acciones asistidas por IA.

Las pruebas de seguridad, respuesta a incidentes, investigación, red team y resolución de problemas deben mantenerse dentro de un alcance claramente autorizado.

### 4. Uso prohibido

No uses OxideTerm para acceso no autorizado, ataques a credenciales, control remoto encubierto, despliegue de malware, explotación de vulnerabilidades fuera de autorización, evasión de controles de seguridad, bypass de restricciones de red, actividad destructiva, exfiltración de datos o abuso de infraestructura de terceros.

No presentes OxideTerm como un producto certificado de seguridad, criptografía, gobierno, cumplimiento o industria regulada salvo que hayas obtenido independientemente las aprobaciones requeridas.

### 5. Privacidad y datos locales

OxideTerm está diseñado como cliente de escritorio local. Por defecto, no requiere una cuenta de OxideTerm, no recopila telemetría ni sube contenido de terminal, perfiles de conexión, historial de comandos, claves privadas, credenciales, archivos locales, ajustes, registros o diagnósticos a un servicio operado por OxideTerm.

Las funciones configuradas por el usuario pueden enviar datos a endpoints elegidos por el usuario, incluidos hosts remotos, backends de sincronización en la nube, proveedores de modelos de IA, destinos de plugins, servidores de actualización, servidores proxy o servicios de almacenamiento. Revisa los destinos y el alcance de datos antes de habilitarlas.

### 6. Secretos y materiales compartidos

Las credenciales y claves API deben permanecer en el almacenamiento seguro de la plataforma cuando esté disponible. Antes de compartir registros, capturas, exportaciones, diagnósticos, transcripciones de terminal, prompts de IA o reportes de errores, revisa y redacta contraseñas, claves privadas, tokens, claves API, cookies, usuarios, hosts, direcciones IP, rutas internas e identificadores de la organización.

### 7. Funciones de IA

OxideSens puede usar proveedores de modelos configurados por el usuario, modelos locales, endpoints compatibles con OpenAI, índices de recuperación, políticas de herramientas y contexto seleccionado del espacio de trabajo. La salida de IA puede ser incorrecta, incompleta o insegura. Revisa sugerencias, comandos, ediciones de archivos y acciones de herramientas antes de ejecutarlas.

No envíes claves privadas, contraseñas, secretos de producción, datos confidenciales, datos regulados u otra información sensible a proveedores de modelos salvo que estés autorizado y aceptes sus términos y prácticas de datos.

### 8. Plugins, terceros y herramientas de red

Instala plugins solo desde fuentes de confianza. Según sus capacidades declaradas, los plugins pueden solicitar acceso al contenido del terminal, archivos, recursos de red, estado del espacio de trabajo, ajustes o API del host.

El reenvío de puertos, proxy jump, proxy ascendente, Raw TCP y Raw UDP se proporcionan para administración y depuración autorizadas. Eres responsable de asegurar que el enrutamiento, reenvío, proxy y tráfico cumplan con tu autorización, políticas organizativas, términos de servicio y leyes aplicables.

Los servicios de terceros, proveedores de modelos, almacenamiento en la nube, hosts remotos, servicios del sistema operativo, registros de paquetes y plugins se rigen por sus propios términos, licencias, políticas de privacidad, controles de seguridad, disponibilidad y prácticas de tratamiento de datos.

### 9. Responsabilidad del usuario

Eres responsable de revisar operaciones antes de ejecutarlas, proteger secretos, mantener copias de seguridad, probar flujos críticos, cumplir requisitos aplicables y usar OxideTerm solo dentro de límites autorizados.

---

## Italiano

### 1. Scopo

OxideTerm è uno spazio di lavoro desktop local-first per operazioni autorizzate di amministrazione di sistema, debug, accesso remoto, trasferimento file, accesso seriale, desktop remoto, diagnostica di rete e flussi assistiti da IA. Questo avviso spiega uso accettabile, confini della privacy, responsabilità verso terzi e limitazioni di garanzia.

Questo avviso non è consulenza legale. Se hai bisogno di un parere legale per una giurisdizione, distribuzione commerciale, ambiente regolamentato, scenario di esportazione o contratto cliente, consulta un avvocato qualificato.

### 2. Licenza e garanzia

OxideTerm è distribuito sotto GNU General Public License version 3.0 only (GPL-3.0-only). Il testo completo della licenza è nel file `LICENSE` del repository. Se questo avviso entra in conflitto con il testo GPL, il testo GPL prevale per la licenza del software.

OxideTerm è fornito senza garanzia nella massima misura consentita dalla legge applicabile. Non sono garantite disponibilità, correttezza, sicurezza, idoneità a uno scopo specifico, idoneità alla conformità o funzionamento ininterrotto.

### 3. Uso autorizzato

Usa OxideTerm solo su sistemi, dispositivi, reti, account, file e servizi che possiedi, amministri o hai il permesso esplicito di operare. Questo include SSH, SFTP, Telnet, Serial, RDP, VNC, Raw TCP, Raw UDP, port forwarding, proxy jump, modifica file, cloud sync, bundle portabili, plugin e azioni assistite da IA.

Test di sicurezza, risposta agli incidenti, ricerca, red team e troubleshooting devono restare entro un ambito chiaramente autorizzato.

### 4. Uso vietato

Non usare OxideTerm per accesso non autorizzato, attacchi alle credenziali, controllo remoto nascosto, distribuzione di malware, sfruttamento di vulnerabilità fuori autorizzazione, elusione di controlli di sicurezza, aggiramento di restrizioni di rete, attività distruttive, esfiltrazione di dati o abuso di infrastrutture di terzi.

Non presentare OxideTerm come prodotto certificato per sicurezza, crittografia, governo, conformità o settori regolamentati salvo che tu abbia ottenuto autonomamente le approvazioni richieste.

### 5. Privacy e dati locali

OxideTerm è progettato come client desktop locale. Per impostazione predefinita non richiede un account OxideTerm, non raccoglie telemetria e non carica contenuti del terminale, profili di connessione, cronologia comandi, chiavi private, credenziali, file locali, impostazioni, log o diagnostica su un servizio gestito da OxideTerm.

Le funzioni configurate dall'utente possono inviare dati a endpoint scelti dall'utente, inclusi host remoti, backend di cloud sync, provider di modelli IA, target di plugin, server di aggiornamento, server proxy o servizi di archiviazione. Controlla destinazioni e ambito dei dati prima di abilitarle.

### 6. Segreti e materiali condivisi

Credenziali e chiavi API dovrebbero restare nell'archiviazione sicura della piattaforma quando disponibile. Prima di condividere log, screenshot, esportazioni, diagnostica, trascrizioni del terminale, prompt IA o bug report, controlla e oscura password, chiavi private, token, chiavi API, cookie, nomi utente, host, indirizzi IP, percorsi interni e identificatori organizzativi.

### 7. Funzioni IA

OxideSens può usare provider di modelli configurati dall'utente, modelli locali, endpoint compatibili OpenAI, indici di recupero, policy degli strumenti e contesto selezionato del workspace. L'output IA può essere errato, incompleto o non sicuro. Controlla suggerimenti, comandi, modifiche ai file e azioni degli strumenti prima dell'esecuzione.

Non inviare chiavi private, password, segreti di produzione, dati confidenziali, dati regolamentati o altre informazioni sensibili a provider di modelli salvo che tu sia autorizzato e accetti i loro termini e pratiche sui dati.

### 8. Plugin, terzi e strumenti di rete

Installa plugin solo da fonti affidabili. In base alle capacità dichiarate, i plugin possono richiedere accesso a contenuto del terminale, file, risorse di rete, stato del workspace, impostazioni o API host.

Port forwarding, proxy jump, proxy upstream, Raw TCP e Raw UDP sono forniti per amministrazione e debug autorizzati. Sei responsabile di assicurare che routing, forwarding, proxy e traffico rispettino autorizzazione, policy organizzative, termini di servizio e leggi applicabili.

Servizi terzi, provider di modelli, cloud storage, host remoti, servizi del sistema operativo, registri di pacchetti e plugin sono regolati dai rispettivi termini, licenze, informative privacy, controlli di sicurezza, disponibilità e pratiche di trattamento dati.

### 9. Responsabilità dell'utente

Sei responsabile di controllare le operazioni prima dell'esecuzione, proteggere i segreti, mantenere backup, testare i flussi critici, rispettare i requisiti applicabili e usare OxideTerm solo entro limiti autorizzati.

---

## Português do Brasil

### 1. Finalidade

OxideTerm é um espaço de trabalho desktop local-first para administração de sistemas, depuração, acesso remoto, transferência de arquivos, acesso serial, desktop remoto, diagnóstico de rede e fluxos assistidos por IA autorizados. Este aviso explica uso aceitável, limites de privacidade, responsabilidades de terceiros e limitações de garantia.

Este aviso não é aconselhamento jurídico. Se você precisar de uma opinião legal para uma jurisdição, implantação comercial, ambiente regulado, cenário de exportação ou contrato com cliente, consulte um advogado qualificado.

### 2. Licença e garantia

OxideTerm é distribuído sob GNU General Public License version 3.0 only (GPL-3.0-only). O texto completo da licença está no arquivo `LICENSE` do repositório. Se este aviso entrar em conflito com o texto da GPL, o texto da GPL controla o licenciamento do software.

OxideTerm é fornecido sem garantia na máxima medida permitida pela lei aplicável. Não há garantia de disponibilidade, correção, segurança, adequação a um propósito específico, adequação regulatória ou operação ininterrupta.

### 3. Uso autorizado

Use OxideTerm apenas em sistemas, dispositivos, redes, contas, arquivos e serviços que você possui, administra ou tem permissão explícita para operar. Isso inclui SSH, SFTP, Telnet, Serial, RDP, VNC, Raw TCP, Raw UDP, encaminhamento de portas, proxy jump, edição de arquivos, cloud sync, pacotes portáteis, plugins e ações assistidas por IA.

Testes de segurança, resposta a incidentes, pesquisa, red team e solução de problemas devem permanecer dentro de um escopo claramente autorizado.

### 4. Uso proibido

Não use OxideTerm para acesso não autorizado, ataques a credenciais, controle remoto oculto, implantação de malware, exploração de vulnerabilidades fora de autorização, evasão de controles de segurança, bypass de restrições de rede, atividade destrutiva, exfiltração de dados ou abuso de infraestrutura de terceiros.

Não apresente OxideTerm como produto certificado de segurança, criptografia, governo, conformidade ou setor regulado, a menos que você tenha obtido independentemente as aprovações necessárias.

### 5. Privacidade e dados locais

OxideTerm foi projetado como cliente desktop local. Por padrão, ele não exige conta OxideTerm, não coleta telemetria e não envia conteúdo do terminal, perfis de conexão, histórico de comandos, chaves privadas, credenciais, arquivos locais, configurações, logs ou diagnósticos para um serviço operado pela OxideTerm.

Recursos configurados pelo usuário podem enviar dados para endpoints escolhidos pelo usuário, incluindo hosts remotos, backends de cloud sync, provedores de modelos de IA, destinos de plugins, servidores de atualização, servidores proxy ou serviços de armazenamento. Revise destinos e escopo dos dados antes de habilitar esses recursos.

### 6. Segredos e materiais compartilhados

Credenciais e chaves de API devem permanecer no armazenamento seguro da plataforma quando disponível. Antes de compartilhar logs, capturas, exportações, diagnósticos, transcrições de terminal, prompts de IA ou relatórios de erro, revise e remova senhas, chaves privadas, tokens, chaves de API, cookies, nomes de usuário, hosts, endereços IP, caminhos internos e identificadores da organização.

### 7. Recursos de IA

OxideSens pode usar provedores de modelos configurados pelo usuário, modelos locais, endpoints compatíveis com OpenAI, índices de recuperação, políticas de ferramentas e contexto selecionado do workspace. A saída de IA pode estar errada, incompleta ou insegura. Revise sugestões, comandos, edições de arquivos e ações de ferramentas antes da execução.

Não envie chaves privadas, senhas, segredos de produção, dados confidenciais, dados regulados ou outras informações sensíveis a provedores de modelos, a menos que você esteja autorizado e aceite seus termos e práticas de dados.

### 8. Plugins, terceiros e ferramentas de rede

Instale plugins apenas de fontes confiáveis. Dependendo das capacidades declaradas, plugins podem solicitar acesso a conteúdo do terminal, arquivos, recursos de rede, estado do workspace, configurações ou APIs do host.

Encaminhamento de portas, proxy jump, proxy upstream, Raw TCP e Raw UDP são fornecidos para administração e depuração autorizadas. Você é responsável por garantir que roteamento, encaminhamento, proxy e tráfego cumpram sua autorização, políticas organizacionais, termos de serviço e leis aplicáveis.

Serviços de terceiros, provedores de modelos, armazenamento em nuvem, hosts remotos, serviços do sistema operacional, registros de pacotes e plugins são regidos por seus próprios termos, licenças, políticas de privacidade, controles de segurança, disponibilidade e práticas de tratamento de dados.

### 9. Responsabilidade do usuário

Você é responsável por revisar operações antes da execução, proteger segredos, manter backups, testar fluxos críticos, cumprir requisitos aplicáveis e usar OxideTerm apenas dentro de limites autorizados.

---

## Tiếng Việt

### 1. Mục đích

OxideTerm là không gian làm việc vận hành desktop ưu tiên cục bộ cho quản trị hệ thống, gỡ lỗi, truy cập từ xa, truyền tệp, truy cập serial, desktop từ xa, chẩn đoán mạng và luồng công việc có AI hỗ trợ đã được ủy quyền. Thông báo này giải thích cách sử dụng chấp nhận được, ranh giới quyền riêng tư, trách nhiệm với bên thứ ba và giới hạn bảo đảm.

Thông báo này không phải là tư vấn pháp lý. Nếu bạn cần ý kiến pháp lý cho một khu vực pháp lý, triển khai thương mại, môi trường chịu quản lý, tình huống xuất khẩu hoặc hợp đồng khách hàng cụ thể, hãy tham khảo luật sư đủ điều kiện.

### 2. Giấy phép và bảo đảm

OxideTerm được phân phối theo GNU General Public License version 3.0 only (GPL-3.0-only). Toàn văn giấy phép nằm trong tệp `LICENSE` của kho mã. Nếu thông báo này mâu thuẫn với văn bản GPL, văn bản GPL sẽ điều chỉnh việc cấp phép phần mềm.

OxideTerm được cung cấp không kèm bảo đảm trong phạm vi tối đa luật áp dụng cho phép. Không có bảo đảm về tính sẵn sàng, độ chính xác, an toàn, phù hợp cho mục đích cụ thể, phù hợp tuân thủ hoặc hoạt động không gián đoạn.

### 3. Sử dụng được ủy quyền

Chỉ sử dụng OxideTerm trên hệ thống, thiết bị, mạng, tài khoản, tệp và dịch vụ mà bạn sở hữu, quản lý hoặc được phép vận hành rõ ràng. Điều này bao gồm SSH, SFTP, Telnet, Serial, RDP, VNC, Raw TCP, Raw UDP, chuyển tiếp cổng, proxy jump, chỉnh sửa tệp, đồng bộ đám mây, gói di động, plugin và hành động có AI hỗ trợ.

Kiểm thử bảo mật, ứng phó sự cố, nghiên cứu, red team và xử lý sự cố phải nằm trong phạm vi được ủy quyền rõ ràng.

### 4. Sử dụng bị cấm

Không dùng OxideTerm cho truy cập trái phép, tấn công thông tin xác thực, điều khiển từ xa che giấu, triển khai mã độc, khai thác lỗ hổng ngoài phạm vi cho phép, né tránh kiểm soát bảo mật, vượt hạn chế mạng, hoạt động phá hoại, rò rỉ dữ liệu hoặc lạm dụng hạ tầng bên thứ ba.

Không quảng bá OxideTerm là sản phẩm đã được chứng nhận về bảo mật, mật mã, chính phủ, tuân thủ hoặc ngành chịu quản lý trừ khi bạn đã độc lập có được các phê duyệt cần thiết.

### 5. Quyền riêng tư và dữ liệu cục bộ

OxideTerm được thiết kế như một ứng dụng desktop cục bộ. Theo mặc định, ứng dụng không yêu cầu tài khoản OxideTerm, không thu thập telemetry và không tải nội dung terminal, hồ sơ kết nối, lịch sử lệnh, khóa riêng, thông tin xác thực, tệp cục bộ, cài đặt, nhật ký hoặc chẩn đoán lên dịch vụ do OxideTerm vận hành.

Các tính năng do người dùng cấu hình có thể gửi dữ liệu đến endpoint do người dùng chọn, bao gồm máy chủ từ xa, backend đồng bộ đám mây, nhà cung cấp mô hình AI, mục tiêu plugin, máy chủ cập nhật, máy chủ proxy hoặc dịch vụ lưu trữ. Hãy xem lại đích đến và phạm vi dữ liệu trước khi bật các tính năng đó.

### 6. Bí mật và tài liệu chia sẻ

Thông tin xác thực và API key nên được giữ trong kho bảo mật của nền tảng khi được hỗ trợ. Trước khi chia sẻ nhật ký, ảnh chụp, gói xuất, chẩn đoán, bản ghi terminal, prompt AI hoặc báo cáo lỗi, hãy kiểm tra và che mật khẩu, khóa riêng, token, API key, cookie, tên người dùng, tên máy chủ, địa chỉ IP, đường dẫn nội bộ và định danh của tổ chức.

### 7. Tính năng AI

OxideSens có thể dùng nhà cung cấp mô hình do người dùng cấu hình, mô hình cục bộ, endpoint tương thích OpenAI, chỉ mục truy xuất, chính sách công cụ và ngữ cảnh workspace được chọn. Đầu ra AI có thể sai, không đầy đủ hoặc không an toàn. Hãy xem lại gợi ý, lệnh, chỉnh sửa tệp và hành động công cụ trước khi thực thi.

Không gửi khóa riêng, mật khẩu, bí mật sản xuất, dữ liệu mật, dữ liệu chịu quản lý hoặc thông tin nhạy cảm khác cho nhà cung cấp mô hình trừ khi bạn được phép và chấp nhận điều khoản cùng cách xử lý dữ liệu của họ.

### 8. Plugin, bên thứ ba và công cụ mạng

Chỉ cài plugin từ nguồn bạn tin tưởng. Tùy theo năng lực khai báo, plugin có thể yêu cầu truy cập nội dung terminal, tệp, tài nguyên mạng, trạng thái workspace, cài đặt hoặc API máy chủ.

Chuyển tiếp cổng, proxy jump, proxy upstream, Raw TCP và Raw UDP được cung cấp cho quản trị và gỡ lỗi đã được ủy quyền. Bạn chịu trách nhiệm bảo đảm định tuyến, chuyển tiếp, proxy và hành vi lưu lượng tuân thủ phạm vi ủy quyền, chính sách tổ chức, điều khoản dịch vụ và luật áp dụng.

Dịch vụ bên thứ ba, nhà cung cấp mô hình, lưu trữ đám mây, máy chủ từ xa, dịch vụ hệ điều hành, registry gói và plugin chịu sự điều chỉnh của điều khoản, giấy phép, chính sách quyền riêng tư, kiểm soát bảo mật, tính sẵn sàng và cách xử lý dữ liệu riêng của họ.

### 9. Trách nhiệm của người dùng

Bạn chịu trách nhiệm xem lại thao tác trước khi thực thi, bảo vệ bí mật, duy trì bản sao lưu, kiểm thử luồng quan trọng, tuân thủ yêu cầu áp dụng và chỉ sử dụng OxideTerm trong phạm vi được ủy quyền.
