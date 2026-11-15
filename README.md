# Gearline — 備品予約・貸出管理デモ

> 個人ポートフォリオ用のフルスタックWebアプリケーションです。実際の業務システムとして稼働した実績を示すものではありません。

備品の空き状況を確認し、期間を指定して予約を申請できます。管理者は備品を登録・編集・利用停止し、予約を承認して貸出・返却まで進められます。予約の重複、権限、状態遷移、監査履歴をフロントエンドだけに頼らずサーバーとDBで扱うことを主題にしています。

## 機能

- 備品カタログ、詳細、期間内の予約済み時間帯を表示
- メンバーの予約申請と、開始前の予約キャンセル
- 管理者による備品登録、編集、利用可能・整備中・利用停止の切り替え
- 予約の承認、却下、貸出、返却
- 予約一覧のロール別表示と管理者の監査ログ
- 予約期間を半開区間 `[開始, 終了)` として扱い、隣接時間帯は予約可能
- 同じ備品の重複予約をPostgreSQLの排他制約で拒否
- 利用者ごとの `Idempotency-Key` によって、タイムアウト後の再試行を同じ予約へ収束
- 状態変更と監査イベントを同じDBトランザクションに記録
- OpenAPI 3.1仕様を `/openapi.yaml` で公開

## 使用技術

| 領域 | 技術 |
| --- | --- |
| API | Rust 1.100、Axum 0.8.9、SQLx 0.9.0、Tokio |
| データベース | PostgreSQL 18、`btree_gist` |
| Web | React 19.3、Vite 8.3、TypeScript 7 |
| テスト | Rust unit/integration tests、Vitest |
| 自動化 | GitHub Actions、Docker Buildx、GitHub Container Registry |
| ローカル実行 | Docker Compose |

Rust依存関係は `Cargo.lock`、JavaScript依存関係は `frontend/pnpm-lock.yaml` で固定しています。Rustツールチェーンは `rust-toolchain.toml` で固定します。上記の安定版を選び、CIとコンテナビルドも同じRustバージョンを使います。SQLxはコンパイル時DB接続を使わず、移行ファイルとバインド付きクエリを実行時に使うため、ローカルのDB起動を簡単にしています。

## 構成

```mermaid
flowchart LR
  Browser[React + Vite] -->|same-origin /api| Web[Nginx web container]
  Web -->|proxy| API[Axum REST API]
  API -->|SQLx| DB[(PostgreSQL 18)]
  API -->|same transaction| Audit[(Audit events)]
```

主なファイルは以下です。

- `src/` — APIルート、セッション認証、入力検証、予約状態遷移
- `migrations/` — スキーマ、DB制約、デモ用ユーザーと備品データ
- `frontend/src/` — React UI、APIクライアント、予約ルールの表示テスト
- `openapi.yaml` — REST API仕様。Axumから `/openapi.yaml` としても配信
- `tests/reservations.rs` — PostgreSQLを使うAPI統合テスト
- `docs/adr/` — 同時実行・認証認可・状態遷移の設計判断
- `.github/workflows/` — PR/mainの検査とタグ時のコンテナ公開

## データモデルとAPI

`users` と `sessions` はデモ認証を支えます。`equipment` は1台ずつ管理する備品、`reservations` は利用期間と状態、`idempotency_keys` は再送キー、`audit_events` は操作履歴を保持します。

`reservations` の排他制約は、同じ `equipment_id` に対して `pending`、`approved`、`checked_out` の期間が重なる行を拒否します。`tstzrange(..., '[)')` により、終了時刻と次の予約開始時刻が同じ予約は重複扱いしません。

主なAPI:

- `GET /api/equipment`, `GET /api/equipment/{id}`, `GET /api/equipment/{id}/availability`
- `POST /api/reservations`, `GET /api/reservations`
- `POST /api/reservations/{id}/cancel|approve|reject|checkout|return`
- `POST /api/equipment`, `PATCH /api/equipment/{id}`
- `GET /api/audit`（管理者のみ）
- `GET /openapi.yaml`

エラーは `application/problem+json` で返し、`code` をクライアント向けの安定した識別子として使います。詳細は [OpenAPI仕様](./openapi.yaml) を参照してください。

## ローカル起動

前提: Docker Desktop等のDocker Compose v2、Git。

```powershell
docker compose up --build
```

ブラウザーで [http://localhost:8080](http://localhost:8080) を開き、メンバーまたは管理者のデモログインを選びます。API仕様は [http://localhost:8080/openapi.yaml](http://localhost:8080/openapi.yaml) です。初回起動ではDBがhealthyになったあと、APIがマイグレーションとデモデータを準備します。停止は `docker compose down`、DBデータを含めて初期化する場合は `docker compose down -v` です。

初期接続情報 `gear/gear` はこのCompose環境だけのローカル用プレースホルダーです。実在するアカウントや秘密情報は含めないでください。データベース接続先を変える場合は `DATABASE_URL` などを環境変数で上書きできます。`.env.example` を参照してください。

個別に起動する場合は、Rust 1.100、Node.js 24、pnpm 11を用意し、PostgreSQL 18を起動します。

```powershell
docker compose up -d db
$env:DATABASE_URL = "postgres://gear:gear@localhost:5432/equipment_reservation"
$env:APP_ENV = "development"
$env:DEMO_AUTH_ENABLED = "true"
cargo run
```

別ターミナルで:

```powershell
cd frontend
pnpm install --frozen-lockfile
pnpm dev
```

フロントエンド開発サーバーは `/api` と `/openapi.yaml` を `localhost:8080` のAPIへプロキシします。

## テスト・ビルド

PostgreSQLを起動してからRustのフォーマット、Lint、unit/integration testsを実行します。

```powershell
docker compose up -d db
$env:DATABASE_URL = "postgres://gear:gear@localhost:5432/equipment_reservation"
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

フロントエンドは `frontend/` で実行します。

```powershell
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test
pnpm build
```

統合テストは `TEST_DATABASE_URL`（なければ `DATABASE_URL`）で指定した**破棄可能なテストDB**にマイグレーションとテストデータを作成します。既存の重要なDBを指定しないでください。CIでは使い捨てPostgreSQL serviceを使います。

## CI/CD

Pull Requestと`main`へのpushでGitHub Actionsが次を実行します。

- `cargo fmt --check`、Clippy、PostgreSQLを使ったRust unit/integration tests
- pnpm install、TypeScript型チェック、Vitest、Vite build
- APIとWebのDockerイメージを別々にbuildして確認（pushなし）

`v*`タグのpushでAPIとWebのイメージをGitHub Container Registryへ公開します。リリースworkflowは`contents: read`と`packages: write`だけを持ち、`GITHUB_TOKEN`をGitHub Actionsから受け取ります。実環境へのデプロイは行いません。

## 設計判断と代替案

- **期間の競合**: アプリ側の事前確認だけでは同時リクエストが競合するため、DBのGiST排他制約を最終防衛線にしました。行ロックだけに頼る案より、将来追加される書き込み経路にも制約が適用されます。
- **二重送信**: 利用者と冪等キーの組を一意にし、要求内容の指紋も比較します。同じキーで異なる要求が来た場合は409を返します。
- **状態変更と監査**: 予約行をロックし、遷移を検証してから状態・バージョンと監査イベントを同一トランザクションで更新します。曖昧な汎用ステータス更新APIは設けません。
- **認証**: 開発用ログインはPostgreSQLに保存したランダムセッションIDをHttpOnly cookieで渡します。ロールはサーバー側で毎回読み、UI上の表示制御を権限境界にしません。
- 詳しい判断と代替案は [ADR 0001](./docs/adr/0001-reservation-integrity.md) と [ADR 0002](./docs/adr/0002-demo-auth-and-lifecycle.md) に記載しています。

## デモの制約と実運用に必要なこと

このデモは数量ではなく個別の1台を予約するモデルで、メール通知、リマインダー、複数拠点・タイムゾーンポリシー、管理者交代、データ保持・削除、監視やバックアップ、ユーザー招待は実装していません。デモログインでは利用者がロールを選択できます。`APP_ENV` が `development` でない場合は開発用ログインが無効です。

実運用するには、OIDC等の組織認証、HTTPSとSecure cookie、CSRF対策、セッション失効・ローテーション、監査ログ保持とアクセス方針、秘密管理、バックアップ・復旧、監視・アラート、脆弱性対応、運用手順、アクセシビリティやブラウザー検証を追加し、実際の組織ルールに合わせて容量・承認・キャンセルポリシーを検証する必要があります。
