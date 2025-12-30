# simple-pr (Jira & Bitbucket PR Automator)

PR을 지라 이슈 기반으로 간단하게 생성하거나, find로 보고용 형식으로 출력하는 cli

## 🚀 주요 기능

- **브랜치 검색 및 필터링**: 키워드를 통해 Bitbucket 브랜치를 빠르게 검색합니다.
- **Jira 연동**: 브랜치 이름에서 Jira 키를 추출하여 해당 이슈의 요약(Summary) 정보를 자동으로 가져옵니다.
- **PR 자동 생성**: 소스 브랜치에서 타겟 브랜치로, Jira 이슈 정보가 포함된 제목의 PR을 즉시 생성합니다.
- **Markdown 출력**: 결과물을 Markdown 형식으로 제공하여 문서화에 용이합니다.

## 🛠️ 설치 및 설정

### 요구 사항

- **Rust**: edition 2024 이상
- **mise** (권장): 도구 버전 관리용

### 환경 변수 설정

.mise.template.toml에 env를 기입하고 파일 명을 .mise.toml로 변경합니다

## 📖 사용 방법

### 1. 브랜치 및 Jira 이슈 찾기 (`find`)

특정 키워드가 포함된 브랜치를 검색하고 연결된 Jira 정보를 확인합니다.

```bash
cargo run -- bitbucket find --from PROJ-123
```

**출력 예시:**

```text
🔍 Searching branches for 'PROJ-123' (filter: None)...
[PROJ-123](https://your-domain.atlassian.net/browse/PROJ-123) 사용자 로그인 시 세션 만료 오류 수정 [PR](https://api.bitbucket.org/2.0/repositories/your_workspace/your_repo/pullrequests/101)
```

### 2. PR 생성 (`create`)

소스 브랜치에서 타겟 브랜치(예: `dev`)로 PR을 생성합니다.

```bash
# 단일 PR 생성
cargo run -- bitbucket create --from PROJ-123 --to dev

# 여러 브랜치 한 번에 처리
cargo run -- bitbucket create --from PROJ-123 PROJ-124 --to dev
```

**출력 예시:**

```text
🔍 Finding target branch for 'dev'...
🎯 Target Branch: dev

Processing PROJ-123 ...
✅ Created: [feature/PROJ-123-login-fix] -> [dev]

🚀 PR Summary:
[PROJ-123](https://your-domain.atlassian.net/browse/PROJ-123) 사용자 로그인 시 세션 만료 오류 수정 [PR](https://api.bitbucket.org/2.0/repositories/your_workspace/your_repo/pullrequests/102)
```

*참고: `--to` 인자에 브랜치명의 일부(예: `dev`)만 입력해도 가장 유사한 브랜치를 자동으로 찾아 타겟으로 지정합니다.*

## 🏗️ 개발 정보

이 프로젝트는 **CursorRIPER♦Σ Lite** 프레임워크를 사용하여 관리됩니다. 상세한 설계 결정 사항 및 기술 스택 정보는 `memory-bank/` 디렉토리를 참조하세요.

- `memory-bank/projectbrief.md`: 프로젝트 개요 및 요구사항
- `memory-bank/systemPatterns.md`: 시스템 아키텍처 및 디자인 결정
- `memory-bank/techContext.md`: 기술 스택 및 환경 설정
