---
name: frontend-nextjs
description: |
  Implements Next.js 15 frontends with App Router, Server Components, and TypeScript.
  Use when building pages, components, data fetching, or client interactions.
  Triggers on: "Next.js", "frontend", "React", "component", "page", "Server Component".
allowed-tools: Read, Grep, Glob, Write, Edit, Bash
---

# Frontend Next.js Agent

Builds production-ready Next.js 15.5.9 applications with App Router and Server Components.

## Version Matrix (December 2025)

| Package | Version | Notes |
|---------|---------|-------|
| Next.js | 15.5.9 | App Router stable |
| React | 19.x | Server Components default |
| TypeScript | 5.x | Strict mode enabled |
| Node.js | 22.x | LTS, use Alpine for Docker |

## Project Initialization

**IMPORTANT**: Always use `nvm` for Node.js version management and `npx` for scaffolding. Never manually create package.json.

### Step 1: Set Node.js Version

```bash
# Load nvm (required in scripts)
source ~/.nvm/nvm.sh

# Use Node.js 22 LTS
nvm use 22 || nvm install 22

# Verify version
node --version  # Should show v22.x.x
```

### Step 2: Create Next.js Project (New Project)

```bash
# Navigate to output directory
cd solution

# Create Next.js app with App Router (installs deps automatically)
npx create-next-app@latest frontend \
  --typescript \
  --tailwind \
  --eslint \
  --app \
  --src-dir=false \
  --import-alias="@/*" \
  --use-npm \
  --yes

# Enter project directory
cd frontend

# Verify dependencies installed
npm list --depth=0
```

### For Existing Project (package.json exists)

```bash
cd solution/frontend

# Load nvm and set Node version
source ~/.nvm/nvm.sh
nvm use 22 || nvm install 22

# Install all dependencies (fetches latest compatible)
npm install

# Verify installation
npm list --depth=0
```

### Step 3: Add Additional Dependencies (if needed)

```bash
# Add runtime dependencies
npm install <package-name>

# Add dev dependencies
npm install --save-dev <package-name>
```

**CRITICAL**: Always run `source ~/.nvm/nvm.sh && nvm use 22` before npm commands, and `npm install` when node_modules is missing.

### Step 4: Configure for Docker (standalone output)

Edit `next.config.ts` to enable standalone output:

```typescript
// next.config.ts
import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  output: 'standalone',
};

export default nextConfig;
```

### Step 5: Create Additional Directories

```bash
# Create component structure
mkdir -p components/{ui,inventory}
mkdir -p lib types

# Create placeholder files
touch lib/api.ts lib/utils.ts types/index.ts
```

## Key Concepts

### Server Components (Default)

- Run on the server only
- Can directly access databases, file system, etc.
- Can use `async/await` at component level
- Smaller bundle size (no client JS shipped)
- **Cannot use**: `useState`, `useEffect`, event handlers, browser APIs

### Client Components

- Add `'use client'` directive at top of file
- Run on both server (SSR) and client
- **Can use**: React hooks, event handlers, browser APIs
- **Cannot use**: Async component functions

### Decision Rule

```
Does this component need interactivity?
├── No → Server Component (default)
└── Yes
    └── Does it need state or event handlers?
        ├── Yes → Client Component ('use client')
        └── No → Server Component with client child
```

## Project Structure

```
frontend/
├── app/
│   ├── layout.tsx           # Root layout
│   ├── page.tsx             # Home page
│   ├── globals.css
│   ├── loading.tsx          # Global loading UI
│   ├── error.tsx            # Global error UI
│   ├── {route}/
│   │   ├── page.tsx         # Route page
│   │   ├── layout.tsx       # Route layout (optional)
│   │   ├── loading.tsx      # Route loading (optional)
│   │   └── error.tsx        # Route error (optional)
│   └── api/                 # API routes (rarely needed)
├── components/
│   ├── ui/                  # Reusable UI components
│   └── {domain}/            # Domain-specific components
├── lib/
│   ├── api.ts               # API client
│   └── utils.ts             # Utility functions
├── types/
│   └── index.ts             # TypeScript types
├── next.config.js
├── tailwind.config.js
├── tsconfig.json
└── package.json
```

## Core Patterns

### Root Layout

```tsx
// app/layout.tsx
import type { Metadata } from 'next';
import './globals.css';

export const metadata: Metadata = {
  title: 'App Name',
  description: 'App description',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body className="min-h-screen bg-background">
        <main className="container mx-auto px-4 py-8">
          {children}
        </main>
      </body>
    </html>
  );
}
```

### Server Component Page with Data Fetching

```tsx
// app/items/page.tsx
// Server Component - can fetch data directly

import { ItemList } from '@/components/items/item-list';

const API_URL = process.env.API_URL || 'http://localhost:8000';

async function getItems() {
  const res = await fetch(`${API_URL}/api/v1/items`, {
    next: { revalidate: 60 }, // ISR: revalidate every 60s
  });

  if (!res.ok) {
    throw new Error('Failed to fetch items');
  }

  return res.json();
}

export default async function ItemsPage() {
  const items = await getItems();

  return (
    <div>
      <h1 className="text-2xl font-bold mb-4">Items</h1>
      <ItemList items={items} />
    </div>
  );
}
```

### Client Component with Interactivity

```tsx
// components/items/item-list.tsx
'use client';

import { useState } from 'react';
import { Item } from '@/types';

interface ItemListProps {
  items: Item[];
}

export function ItemList({ items: initialItems }: ItemListProps) {
  const [items, setItems] = useState(initialItems);
  const [filter, setFilter] = useState('');

  const filteredItems = items.filter((item) =>
    item.name.toLowerCase().includes(filter.toLowerCase())
  );

  return (
    <div>
      <input
        type="text"
        placeholder="Filter items..."
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        className="border px-3 py-2 rounded mb-4 w-full"
      />
      <ul className="space-y-2">
        {filteredItems.map((item) => (
          <li key={item.id} className="border p-3 rounded">
            {item.name} - Qty: {item.quantity}
          </li>
        ))}
      </ul>
    </div>
  );
}
```

### Server Actions (Mutations)

```tsx
// app/actions/items.ts
'use server';

import { revalidatePath } from 'next/cache';

const API_URL = process.env.API_URL || 'http://localhost:8000';

export async function createItem(formData: FormData) {
  const name = formData.get('name') as string;
  const quantity = Number(formData.get('quantity'));

  const res = await fetch(`${API_URL}/api/v1/items`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, quantity }),
  });

  if (!res.ok) {
    throw new Error('Failed to create item');
  }

  revalidatePath('/items');
  return res.json();
}

export async function deleteItem(id: number) {
  const res = await fetch(`${API_URL}/api/v1/items/${id}`, {
    method: 'DELETE',
  });

  if (!res.ok) {
    throw new Error('Failed to delete item');
  }

  revalidatePath('/items');
}
```

### Form with Server Action

```tsx
// components/items/create-item-form.tsx
'use client';

import { useFormStatus } from 'react-dom';
import { createItem } from '@/app/actions/items';

function SubmitButton() {
  const { pending } = useFormStatus();

  return (
    <button
      type="submit"
      disabled={pending}
      className="bg-blue-500 text-white px-4 py-2 rounded disabled:opacity-50"
    >
      {pending ? 'Creating...' : 'Create Item'}
    </button>
  );
}

export function CreateItemForm() {
  return (
    <form action={createItem} className="space-y-4">
      <div>
        <label htmlFor="name" className="block text-sm font-medium">
          Name
        </label>
        <input
          type="text"
          id="name"
          name="name"
          required
          className="border px-3 py-2 rounded w-full"
        />
      </div>
      <div>
        <label htmlFor="quantity" className="block text-sm font-medium">
          Quantity
        </label>
        <input
          type="number"
          id="quantity"
          name="quantity"
          required
          min="0"
          className="border px-3 py-2 rounded w-full"
        />
      </div>
      <SubmitButton />
    </form>
  );
}
```

### Loading UI

```tsx
// app/items/loading.tsx
export default function Loading() {
  return (
    <div className="space-y-4">
      <div className="h-8 w-48 bg-gray-200 animate-pulse rounded" />
      <div className="space-y-2">
        {[...Array(5)].map((_, i) => (
          <div key={i} className="h-16 bg-gray-200 animate-pulse rounded" />
        ))}
      </div>
    </div>
  );
}
```

### Error UI

```tsx
// app/items/error.tsx
'use client';

export default function Error({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <div className="text-center py-8">
      <h2 className="text-xl font-bold text-red-600 mb-4">
        Something went wrong!
      </h2>
      <p className="text-gray-600 mb-4">{error.message}</p>
      <button
        onClick={reset}
        className="bg-blue-500 text-white px-4 py-2 rounded"
      >
        Try again
      </button>
    </div>
  );
}
```

## API Client Pattern

```tsx
// lib/api.ts
const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8000';

class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

async function fetchApi<T>(
  path: string,
  options?: RequestInit
): Promise<T> {
  const res = await fetch(`${API_URL}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });

  if (!res.ok) {
    throw new ApiError(res.status, `API error: ${res.status}`);
  }

  return res.json();
}

export const api = {
  items: {
    list: () => fetchApi<Item[]>('/api/v1/items'),
    get: (id: number) => fetchApi<Item>(`/api/v1/items/${id}`),
    create: (data: ItemCreate) =>
      fetchApi<Item>('/api/v1/items', {
        method: 'POST',
        body: JSON.stringify(data),
      }),
    update: (id: number, data: ItemUpdate) =>
      fetchApi<Item>(`/api/v1/items/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(data),
      }),
    delete: (id: number) =>
      fetchApi<void>(`/api/v1/items/${id}`, { method: 'DELETE' }),
  },
};
```

## next.config.js

```js
/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'standalone', // For Docker deployment
  experimental: {
    serverActions: {
      bodySizeLimit: '2mb',
    },
  },
};

module.exports = nextConfig;
```

## See Also

- `APP-ROUTER.md` for detailed routing patterns
- `SERVER-ACTIONS.md` for mutation patterns
- `TYPESCRIPT.md` for type generation from OpenAPI
