---
name: devops
description: |
  Deploys applications with Docker, docker-compose, and optional Kubernetes.
  Use when containerizing, setting up local dev, or preparing for deployment.
  Triggers on: "Docker", "container", "deploy", "docker-compose", "Kubernetes".
allowed-tools: Read, Grep, Glob, Write, Edit, Bash
---

# DevOps Agent

Containerizes and deploys applications with production-ready patterns.

## Version Matrix

| Tool | Version | Notes |
|------|---------|-------|
| Docker | 27.x | BuildKit enabled by default |
| Docker Compose | 2.x | V2 syntax |
| PostgreSQL | 17-alpine | For containers |
| Python | 3.13-slim-bookworm | NOT Alpine |
| Node.js | 22-alpine | For frontend |

## Full Project Initialization

**IMPORTANT**: Use proper tooling for each layer. Never manually create config files.

### Complete Setup Sequence

```bash
# 1. Create solution directory
mkdir -p solution && cd solution

# 2. Initialize Backend (see foundation/backend-fastapi for details)
uv init backend --python 3.13
cd backend
uv add fastapi uvicorn[standard] sqlmodel asyncpg pydantic-settings httpx alembic greenlet
uv add --dev pytest pytest-asyncio aiosqlite ruff
mkdir -p app/{api/v1,models,services,scripts} tests/api
cd ..

# 3. Initialize Frontend (see foundation/frontend-nextjs for details)
nvm use 22
npx create-next-app@latest frontend \
  --typescript --tailwind --eslint --app \
  --src-dir=false --import-alias="@/*" --use-npm
cd frontend
mkdir -p components/{ui,inventory} lib types
cd ..

# 4. Create Docker and environment files
touch docker-compose.yml .env.example Makefile README.md
```

### Quick Reference

| Layer | Tool | Init Command |
|-------|------|--------------|
| Python | uv | `uv init <name> --python 3.13` |
| Dependencies (Python) | uv | `uv add <package>` |
| Run Python | uv | `uv run <command>` |
| Node.js version | nvm | `nvm use 22` |
| Next.js | npx | `npx create-next-app@latest <name>` |
| Dependencies (Node) | npm | `npm install <package>` |
| Alembic | uv | `uv run alembic init alembic` |
| Migrations | uv | `uv run alembic upgrade head` |

## Project Structure

```
project/
├── backend/
│   ├── Dockerfile
│   ├── .dockerignore
│   └── ...
├── frontend/
│   ├── Dockerfile
│   ├── .dockerignore
│   └── ...
├── docker-compose.yml
├── docker-compose.override.yml  # Dev overrides
├── .env.example
└── Makefile
```

## docker-compose.yml (Cognita-inspired pattern)

```yaml
# docker-compose.yml
# Pattern: Service health dependencies, network isolation, volume persistence

services:
  db:
    image: postgres:17-alpine
    container_name: ${PROJECT_NAME:-app}-db
    environment:
      POSTGRES_USER: ${DB_USER:-app}
      POSTGRES_PASSWORD: ${DB_PASSWORD:-secret}
      POSTGRES_DB: ${DB_NAME:-app}
    ports:
      - "5432:5432"
    volumes:
      - ./volumes/pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${DB_USER:-app} -d ${DB_NAME:-app}"]
      interval: 10s
      timeout: 5s
      retries: 5
    networks:
      - app-network

  backend:
    build:
      context: ./backend
      dockerfile: Dockerfile
      target: development
    container_name: ${PROJECT_NAME:-app}-backend
    ports:
      - "8000:8000"
    environment:
      DATABASE_URL: postgresql+asyncpg://${DB_USER:-app}:${DB_PASSWORD:-secret}@db:5432/${DB_NAME:-app}
      DEBUG: ${DEBUG:-true}
    depends_on:
      db:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/api/v1/health/ready"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
    volumes:
      - ./backend/app:/app/app  # Hot reload
    command: uvicorn app.main:app --host 0.0.0.0 --port 8000 --reload
    networks:
      - app-network

  frontend:
    build:
      context: ./frontend
      dockerfile: Dockerfile
      target: development
    container_name: ${PROJECT_NAME:-app}-frontend
    ports:
      - "3000:3000"
    environment:
      NEXT_PUBLIC_API_URL: http://localhost:8000
    depends_on:
      backend:
        condition: service_healthy
    volumes:
      - ./frontend:/app
      - /app/node_modules
      - /app/.next
    networks:
      - app-network

networks:
  app-network:
    driver: bridge

# Volumes stored in ./volumes/ for easy inspection
# Add to .gitignore: volumes/
```

## Backend Dockerfile (FastAPI + uv)

```dockerfile
# backend/Dockerfile
FROM python:3.13-slim-bookworm AS base

# Install uv for fast dependency management
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

WORKDIR /app

# Copy dependency files
COPY pyproject.toml ./

# ──────────────────────────────────────────────────────────
# Development stage
FROM base AS development

# Install all dependencies including dev
RUN --mount=type=cache,target=/root/.cache/uv \
    uv pip install --system -e ".[dev]"

COPY . .

EXPOSE 8000
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000", "--reload"]

# ──────────────────────────────────────────────────────────
# Production build stage
FROM base AS builder

# Install production dependencies only
RUN --mount=type=cache,target=/root/.cache/uv \
    uv pip install --system .

COPY . .

# ──────────────────────────────────────────────────────────
# Production runtime stage
FROM python:3.13-slim-bookworm AS production

WORKDIR /app

# Create non-root user
RUN adduser --system --uid 1001 appuser

# Copy installed packages from builder
COPY --from=builder /usr/local/lib/python3.13/site-packages /usr/local/lib/python3.13/site-packages
COPY --from=builder /usr/local/bin/uvicorn /usr/local/bin/uvicorn

# Copy application code
COPY --from=builder --chown=appuser:appuser /app /app

USER appuser

EXPOSE 8000

# Use exec form for proper signal handling
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000"]
```

---

## Node.js Stack (Express.js + SQLite)

### docker-compose.yml (Node.js variant)

```yaml
# docker-compose.yml for Node.js + SQLite
services:
  backend:
    build:
      context: ./backend
      dockerfile: Dockerfile
      target: development
    container_name: ${PROJECT_NAME:-app}-backend
    ports:
      - "3001:3001"
    environment:
      DATABASE_PATH: /app/data/database.sqlite
      LOG_LEVEL: ${LOG_LEVEL:-INFO}
    volumes:
      - ./backend/src:/app/src
      - ./backend/data:/app/data
    command: npx tsx watch src/index.ts
    networks:
      - app-network

  frontend:
    build:
      context: ./frontend
      dockerfile: Dockerfile
      target: development
    container_name: ${PROJECT_NAME:-app}-frontend
    ports:
      - "3000:3000"
    environment:
      VITE_API_URL: http://localhost:3001
    depends_on:
      - backend
    volumes:
      - ./frontend:/app
      - /app/node_modules
    networks:
      - app-network

networks:
  app-network:
    driver: bridge
```

### Backend Dockerfile (Express.js + TypeScript)

```dockerfile
# backend/Dockerfile
FROM node:22-alpine AS base
WORKDIR /app

# ─────────────────────────────────────────
# Dependencies stage
FROM base AS deps
COPY package.json package-lock.json* ./
RUN npm ci --only=production

# ─────────────────────────────────────────
# Development stage
FROM base AS development
COPY package.json package-lock.json* ./
RUN npm ci
COPY . .
EXPOSE 3001
CMD ["npx", "tsx", "watch", "src/index.ts"]

# ─────────────────────────────────────────
# Builder stage
FROM base AS builder
COPY package.json package-lock.json* ./
RUN npm ci
COPY . .
RUN npm run build

# ─────────────────────────────────────────
# Production stage
FROM base AS production
ENV NODE_ENV=production

RUN adduser --system --uid 1001 appuser

COPY --from=deps --chown=appuser:appuser /app/node_modules ./node_modules
COPY --from=builder --chown=appuser:appuser /app/dist ./dist
COPY --from=builder --chown=appuser:appuser /app/package.json ./

# Create data directory for SQLite
RUN mkdir -p /app/data && chown appuser:appuser /app/data

USER appuser
EXPOSE 3001
CMD ["node", "dist/index.js"]
```

---

## Frontend Dockerfile (Next.js)

```dockerfile
# frontend/Dockerfile
FROM node:22-alpine AS base

WORKDIR /app

# ──────────────────────────────────────────────────────────
# Dependencies stage
FROM base AS deps

COPY package.json package-lock.json* ./
RUN npm ci

# ──────────────────────────────────────────────────────────
# Development stage
FROM base AS development

COPY --from=deps /app/node_modules ./node_modules
COPY . .

ENV NODE_ENV=development
EXPOSE 3000

CMD ["npm", "run", "dev"]

# ──────────────────────────────────────────────────────────
# Build stage
FROM base AS builder

COPY --from=deps /app/node_modules ./node_modules
COPY . .

ENV NEXT_TELEMETRY_DISABLED=1
RUN npm run build

# ──────────────────────────────────────────────────────────
# Production stage
FROM base AS production

ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1

RUN addgroup --system --gid 1001 nodejs
RUN adduser --system --uid 1001 nextjs

# Copy built application (standalone output)
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static
COPY --from=builder --chown=nextjs:nodejs /app/public ./public

USER nextjs

EXPOSE 3000
ENV PORT=3000

# Direct node execution for proper signal handling
CMD ["node", "server.js"]
```

## Frontend Dockerfile (React + Vite with Nginx)

```dockerfile
# frontend/Dockerfile (for React + Vite SPA)
FROM node:22-alpine AS base
WORKDIR /app

# ─────────────────────────────────────────
# Dependencies stage
FROM base AS deps
COPY package.json package-lock.json* ./
RUN npm ci

# ─────────────────────────────────────────
# Development stage
FROM base AS development
COPY --from=deps /app/node_modules ./node_modules
COPY . .
ENV NODE_ENV=development
EXPOSE 3000
CMD ["npm", "run", "dev", "--", "--host"]

# ─────────────────────────────────────────
# Builder stage
FROM base AS builder
COPY --from=deps /app/node_modules ./node_modules
COPY . .
ENV NODE_ENV=production
RUN npm run build

# ─────────────────────────────────────────
# Production stage (Nginx)
FROM nginx:alpine AS production

COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 3000
CMD ["nginx", "-g", "daemon off;"]
```

### nginx.conf (for React + Vite SPA)

```nginx
server {
    listen 3000;
    root /usr/share/nginx/html;
    index index.html;

    # SPA routing - serve index.html for all routes
    location / {
        try_files $uri $uri/ /index.html;
    }

    # Cache static assets
    location ~* \.(js|css|png|jpg|jpeg|gif|ico|svg)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }

    # API proxy (if needed)
    location /api {
        proxy_pass http://backend:3001;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
    }
}
```

---

## .dockerignore Files

### backend/.dockerignore

```
.git
.gitignore
.env
.env.*
__pycache__
*.pyc
*.pyo
.pytest_cache
.coverage
htmlcov
.mypy_cache
.ruff_cache
*.egg-info
dist
build
.venv
venv
```

### frontend/.dockerignore

```
.git
.gitignore
.env
.env.*
node_modules
.next
.turbo
coverage
.nyc_output
*.log
```

## Makefile

```makefile
.PHONY: dev build test clean migrate seed logs

# Development
dev:
	docker compose up --build

dev-detach:
	docker compose up --build -d

logs:
	docker compose logs -f

stop:
	docker compose stop

# Database
migrate:
	docker compose exec backend uv run alembic upgrade head

seed:
	docker compose exec backend uv run python -m app.scripts.seed

reset-db:
	docker compose down -v
	docker compose up -d db
	sleep 3
	$(MAKE) migrate
	$(MAKE) seed

# Testing
test:
	docker compose exec backend uv run pytest -v

test-frontend:
	docker compose exec frontend npm test

# Production build
build:
	docker compose -f docker-compose.yml build

# Cleanup
clean:
	docker compose down -v --remove-orphans
	docker system prune -f

# Utility
shell-backend:
	docker compose exec backend bash

shell-frontend:
	docker compose exec frontend sh

shell-db:
	docker compose exec db psql -U app -d app
```

## Environment Variables

### .env.example

```bash
# Database
DB_USER=app
DB_PASSWORD=secret
DB_NAME=app

# Backend
DATABASE_URL=postgresql+asyncpg://app:secret@db:5432/app
SECRET_KEY=change-me-in-production

# Frontend
NEXT_PUBLIC_API_URL=http://localhost:8000
```

## Health Checks

### Backend Health Endpoints

Already included in backend-fastapi skill. Ensure `/health/live` and `/health/ready` exist.

### Docker Compose Health Check

```yaml
backend:
  healthcheck:
    test: ["CMD", "curl", "-f", "http://localhost:8000/api/v1/health/ready"]
    interval: 10s
    timeout: 5s
    retries: 3
    start_period: 30s
```

## CI/CD with GitHub Actions

### Directory Structure
```
solution/
├── .github/
│   └── workflows/
│       ├── ci.yml           # Continuous Integration
│       ├── deploy.yml       # Deployment (optional)
│       └── release.yml      # Release automation (optional)
```

### CI Workflow (ci.yml)

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  # ─────────────────────────────────────────
  # Backend Tests
  # ─────────────────────────────────────────
  backend-test:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: backend

    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_USER: test
          POSTGRES_PASSWORD: test
          POSTGRES_DB: test
        ports:
          - 5432:5432
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v4
        with:
          version: "latest"

      - name: Set up Python
        run: uv python install 3.13

      - name: Install dependencies
        run: uv sync

      - name: Run linting
        run: uv run ruff check .

      - name: Run type checking
        run: uv run mypy app --ignore-missing-imports

      - name: Run tests
        env:
          DATABASE_URL: postgresql+asyncpg://test:test@localhost:5432/test
        run: uv run pytest tests/ -v --cov=app --cov-report=xml

      - name: Upload coverage
        uses: codecov/codecov-action@v4
        with:
          file: ./backend/coverage.xml
          fail_ci_if_error: false

  # ─────────────────────────────────────────
  # Frontend Tests
  # ─────────────────────────────────────────
  frontend-test:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: frontend

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: frontend/package-lock.json

      - name: Install dependencies
        run: npm ci

      - name: Run linting
        run: npm run lint

      - name: Run type checking
        run: npm run type-check || true

      - name: Run tests
        run: npm test -- --run

      - name: Build
        run: npm run build

  # ─────────────────────────────────────────
  # Docker Build
  # ─────────────────────────────────────────
  docker-build:
    runs-on: ubuntu-latest
    needs: [backend-test, frontend-test]

    steps:
      - uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Build backend image
        uses: docker/build-push-action@v5
        with:
          context: ./backend
          target: production
          push: false
          tags: backend:test
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Build frontend image
        uses: docker/build-push-action@v5
        with:
          context: ./frontend
          target: production
          push: false
          tags: frontend:test
          cache-from: type=gha
          cache-to: type=gha,mode=max

  # ─────────────────────────────────────────
  # Integration Tests
  # ─────────────────────────────────────────
  integration-test:
    runs-on: ubuntu-latest
    needs: [docker-build]

    steps:
      - uses: actions/checkout@v4

      - name: Start services
        run: docker compose up -d --build

      - name: Wait for services
        run: |
          timeout 60 bash -c 'until curl -s http://localhost:8000/health; do sleep 2; done'

      - name: Run E2E tests
        run: |
          # Health check
          curl -f http://localhost:8000/health

          # Create item
          curl -f -X POST http://localhost:8000/api/v1/items \
            -H "Content-Type: application/json" \
            -d '{"name": "CI Test", "quantity": 1}'

          # List items
          curl -f http://localhost:8000/api/v1/items

      - name: Show logs on failure
        if: failure()
        run: docker compose logs

      - name: Cleanup
        if: always()
        run: docker compose down -v
```

### Deploy Workflow (Optional)

```yaml
# .github/workflows/deploy.yml
name: Deploy

on:
  push:
    branches: [main]
  workflow_dispatch:

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  build-and-push:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write

    steps:
      - uses: actions/checkout@v4

      - name: Log in to Container Registry
        uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Build and push backend
        uses: docker/build-push-action@v5
        with:
          context: ./backend
          target: production
          push: true
          tags: |
            ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}/backend:latest
            ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}/backend:${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Build and push frontend
        uses: docker/build-push-action@v5
        with:
          context: ./frontend
          target: production
          push: true
          tags: |
            ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}/frontend:latest
            ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}/frontend:${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

### CI/CD Checklist

- [ ] `.github/workflows/ci.yml` created
- [ ] Backend tests run in CI
- [ ] Frontend tests run in CI
- [ ] Docker images build successfully
- [ ] Integration tests pass
- [ ] (Optional) Deploy workflow configured

---

## README Template (Cognita-inspired structure)

```markdown
# Project Name

> Brief one-line description of what this project does.

![Demo](docs/images/demo.gif) <!-- Optional: Add demo GIF/screenshot -->

## Why This Project?

[2-3 sentences explaining the problem and how this solution addresses it]

---

## Table of Contents

- [Quick Start](#-quick-start)
- [Features](#-features)
- [Architecture](#-architecture)
- [Development](#-development)
- [What Was Built](#-what-was-built)
- [Key Decisions](#-key-decisions)

---

## 🚀 Quick Start

### Prerequisites

- Docker & Docker Compose
- Make (optional, for convenience commands)

### Run with Docker (Recommended)

\`\`\`bash
# Clone and setup
git clone <repo>
cd <project>
cp .env.example .env

# Start all services
docker compose up --build

# In another terminal, run migrations
docker compose exec backend uv run alembic upgrade head
docker compose exec backend uv run python -m app.scripts.seed
\`\`\`

The app is now running:

| Service | URL |
|---------|-----|
| Frontend | http://localhost:3000 |
| Backend API | http://localhost:8000 |
| API Docs | http://localhost:8000/docs |
| Database | localhost:5432 |

---

## ✨ Features

- **Feature 1**: Description
- **Feature 2**: Description
- **Feature 3**: Description

---

## 🏗️ Architecture

\`\`\`
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Frontend  │────▶│   Backend   │────▶│  Database   │
│  (Next.js)  │     │  (FastAPI)  │     │ (PostgreSQL)│
└─────────────┘     └─────────────┘     └─────────────┘
\`\`\`

See `docs/architecture/` for detailed diagrams.

---

## ⚒️ Development

\`\`\`bash
docker compose up --build        # Start all services with hot reload
docker compose logs -f           # View logs
docker compose stop              # Stop services
docker compose down -v           # Remove containers and volumes

# Run tests
docker compose exec backend uv run pytest -v
docker compose exec frontend npm test
\`\`\`

### Project Structure

\`\`\`
.
├── backend/           # FastAPI application
├── frontend/          # Next.js application
├── docs/              # Architecture, ADRs
│   ├── architecture/
│   └── decisions/
├── docker-compose.yml
└── Makefile
\`\`\`

---

## ✅ What Was Built

- [x] Feature 1 - Description
- [x] Feature 2 - Description
- [x] Feature 3 - Description

## 🚫 What Was Not Built (and Why)

| Feature | Reason | Future Approach |
|---------|--------|-----------------|
| Feature X | Time constraint | Would use approach Y |
| Feature Y | Out of MVP scope | Could add in v2 |

---

## 📋 Key Decisions

See `docs/decisions/` for Architecture Decision Records (ADRs).

| Decision | Rationale |
|----------|-----------|
| PostgreSQL over MongoDB | Relational data with complex joins |
| FastAPI over Django | Async-first, auto OpenAPI docs |

---

## 🤖 AI Tools Disclosure

This project was built with AI assistance. All code was reviewed and tested before submission.
\`\`\`

## Quality Checklist

- [ ] `docker compose up` works from clean clone
- [ ] README has clear setup instructions
- [ ] All services start without errors
- [ ] Health checks pass
- [ ] Migrations apply cleanly
- [ ] Seed data loads successfully
