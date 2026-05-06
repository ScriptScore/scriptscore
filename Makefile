SHELL := /bin/bash

UV_CACHE_DIR ?= /tmp/uv-cache
RUFF_CACHE_DIR ?= /tmp/scriptscore-ruff-cache
MYPY_CACHE_DIR ?= /tmp/scriptscore-mypy-cache
PYTEST_CACHE_DIR ?= /tmp/scriptscore-pytest-cache
COVERAGE_FILE ?= /tmp/scriptscore-cli.coverage
SONAR_HOST_URL ?= https://sonarcloud.io
SONAR_BRANCH_NAME ?=

.PHONY: cli-lint cli-quality cargo-fmt lint-rust lint-frontend coverage-frontend quality-frontend quality-metrics coverage-rust unsafe-report sonar-local quality review-quality license-compliance prepare-desktop-runtime prepare-desktop-portable-python smoke-desktop-runtime package-desktop-linux package-desktop-rpm

cli-lint:
	UV_CACHE_DIR="$(UV_CACHE_DIR)" uv --directory cli run ruff check . --cache-dir "$(RUFF_CACHE_DIR)"
	UV_CACHE_DIR="$(UV_CACHE_DIR)" uv --directory cli run ruff format --check . --cache-dir "$(RUFF_CACHE_DIR)"

cli-quality:
	UV_CACHE_DIR="$(UV_CACHE_DIR)" MYPY_CACHE_DIR="$(MYPY_CACHE_DIR)" uv --directory cli run mypy
	UV_CACHE_DIR="$(UV_CACHE_DIR)" COVERAGE_FILE="$(COVERAGE_FILE)" PYTEST_ADDOPTS="-o cache_dir=$(PYTEST_CACHE_DIR) $$PYTEST_ADDOPTS" uv --directory cli run pytest -q --cov

cargo-fmt:
	cargo fmt --check --manifest-path desktop/src-tauri/Cargo.toml

lint-rust:
	cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --workspace --all-targets --all-features -- -D warnings

lint-frontend:
	npm --prefix desktop/frontend run lint

coverage-frontend:
	npm --prefix desktop/frontend run coverage

quality-frontend:
	npm --prefix desktop/frontend test

quality-metrics:
	python3 -m unittest discover -s desktop/scripts/tests -p 'test_*.py'
	./desktop/scripts/run-rust-code-analysis.sh

coverage-rust:
	./desktop/scripts/run-cargo-tarpaulin.sh

unsafe-report:
	./desktop/scripts/run-cargo-geiger.sh

sonar-local:
	@test -n "$$SONAR_TOKEN" || { echo "SONAR_TOKEN is required"; exit 1; }
	@test -n "$$SONAR_ORGANIZATION" || { echo "SONAR_ORGANIZATION is required"; exit 1; }
	@test -n "$$SONAR_PROJECT_KEY" || { echo "SONAR_PROJECT_KEY is required"; exit 1; }
	@test -f desktop/frontend/coverage/lcov.info || { echo "Missing desktop/frontend/coverage/lcov.info; run make coverage-frontend first"; exit 1; }
	@test -f artifacts/coverage/lcov.info || { echo "Missing artifacts/coverage/lcov.info; run make coverage-rust first"; exit 1; }
	cd desktop/frontend && npm exec svelte-kit sync
	./desktop/scripts/prepare-bundled-runtime.sh
	python3 desktop/scripts/generate_legal_artifacts.py
	@branch_arg=""; \
	if [ -n "$(SONAR_BRANCH_NAME)" ]; then branch_arg="-Dsonar.branch.name=$(SONAR_BRANCH_NAME)"; fi; \
	sonar-scanner \
		-Dsonar.host.url="$(SONAR_HOST_URL)" \
		-Dsonar.organization="$$SONAR_ORGANIZATION" \
		-Dsonar.projectKey="$$SONAR_PROJECT_KEY" \
		-Dsonar.token="$$SONAR_TOKEN" \
		$$branch_arg

prepare-desktop-runtime:
	./desktop/scripts/prepare-bundled-runtime.sh

prepare-desktop-portable-python:
	bash ./desktop/scripts/prepare-portable-python.sh

smoke-desktop-runtime: prepare-desktop-runtime
	./desktop/scripts/smoke-bundled-runtime.sh

package-desktop-linux:
	./desktop/scripts/build-desktop-package.sh appimage

package-desktop-rpm:
	./desktop/scripts/build-desktop-package.sh rpm

license-compliance:
	python3 scripts/check_spdx_headers.py
	python3 desktop/scripts/check_scriptscoreplus_boundary.py
	python3 desktop/scripts/generate_legal_artifacts.py --check

quality: cargo-fmt lint-rust lint-frontend cli-lint cli-quality quality-frontend coverage-frontend coverage-rust quality-metrics unsafe-report license-compliance

review-quality: quality
