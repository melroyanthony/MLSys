---
description: Implement Next.js frontend with TypeScript (Stage 3)
allowed-tools: Read, Glob, Grep, Write, Edit, Bash
---

# Frontend Developer

Implement production-ready Next.js frontend based on architecture specs.

## Tech Stack
- Node.js 22 (via nvm)
- Next.js 15+ with App Router
- TypeScript strict mode
- Tailwind CSS

## Project Setup

```bash
cd solution
source ~/.nvm/nvm.sh
nvm use 22 || nvm install 22

npx create-next-app@latest frontend \
  --typescript --tailwind --eslint --app \
  --src-dir=false --import-alias="@/*" --use-npm --yes

cd frontend
npm list --depth=0
```

## Code Structure
```
frontend/
├── app/
│   ├── layout.tsx       # Root layout
│   ├── page.tsx         # Home page
│   └── [feature]/page.tsx
├── components/          # Reusable components
├── lib/api.ts           # API client
├── types/api.ts         # TypeScript types
└── next.config.ts
```

## Critical Patterns

### TypeScript Types (match backend)
```typescript
export interface Item {
  id: number;
  name: string;
  quantity: number;
}
```

### API Client
```typescript
const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000";

async function fetchApi<T>(endpoint: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}/api/v1${endpoint}`, {
    ...options,
    headers: { "Content-Type": "application/json", ...options?.headers },
  });
  if (!response.ok) throw new Error("API Error");
  return response.json();
}
```

### Client Components
```typescript
"use client";
import { useState, useEffect } from "react";
```

### Docker Config
```typescript
// next.config.ts
const nextConfig = { output: "standalone" };
```

## Implementation Flow
1. Create TypeScript types matching backend
2. Implement API client functions
3. Build components with Tailwind
4. Compose pages
5. Update next.config.ts for Docker

## Run Commands
```bash
npm run dev
npm run build
```

## Input
- OpenAPI spec from `solution/docs/architecture/openapi.yaml`
- Backend API running on localhost:8000
