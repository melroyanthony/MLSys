---
name: backend-fastapi
description: |
  Implements async FastAPI backends with SQLModel, Pydantic v2, and asyncpg.
  Use when building API endpoints, database models, or business logic.
  Triggers on: "FastAPI", "backend", "API", "endpoint", "SQLModel".
allowed-tools: Read, Grep, Glob, Write, Edit, Bash
---

# Backend FastAPI Agent

Builds production-ready async FastAPI applications with SQLModel 0.0.22+ and Pydantic v2.

## Version Matrix (December 2025)

| Package | Version | Notes |
|---------|---------|-------|
| FastAPI | 0.115.x | Async-first |
| SQLModel | 0.0.22+ | Full Pydantic v2 support |
| Pydantic | 2.9.x | `field_validator`, `model_validator` |
| SQLAlchemy | 2.0.x | Required for async |
| asyncpg | 0.29.x | PostgreSQL async driver |
| Alembic | 1.17.x | Use `render_as_batch=True` |
| Python | 3.13.x | Use slim-bookworm for Docker |

## CRITICAL: Always Use `uv` for Python

**NEVER use these commands:**
- ❌ `python3 -m venv venv`
- ❌ `pip install`
- ❌ `pip freeze`

**ALWAYS use `uv` instead:**
- ✅ `uv venv`
- ✅ `uv add <package>`
- ✅ `uv sync`
- ✅ `uv run <command>`

## Project Initialization

**IMPORTANT**: Always use `uv` for Python project scaffolding. Never manually create pyproject.toml.

### Step 1: Initialize Project

```bash
# Navigate to solution directory
cd solution

# Initialize Python project with uv
uv init backend --python 3.13

# Enter project directory
cd backend

# Create virtual environment
uv venv
```

### Step 2: Add Dependencies

```bash
# Core dependencies (fetches latest versions)
uv add fastapi uvicorn[standard] sqlmodel asyncpg pydantic-settings httpx alembic greenlet

# Dev dependencies
uv add --dev pytest pytest-asyncio aiosqlite ruff psycopg2-binary

# Sync to ensure all deps are installed
uv sync
```

### For Existing Project (pyproject.toml exists)

```bash
cd solution/backend

# Create venv if not exists
uv venv

# Install all dependencies
uv sync
```

**CRITICAL**: Always run `uv venv` before adding dependencies, and `uv sync` before running code.

### Step 3: Initialize Alembic

```bash
# Initialize Alembic for migrations
uv run alembic init alembic
```

### Step 4: Create Directory Structure

```bash
# Create app structure
mkdir -p app/{api/v1,models,services,scripts}
touch app/__init__.py app/api/__init__.py app/api/v1/__init__.py
touch app/models/__init__.py app/services/__init__.py

# Create test structure
mkdir -p tests/api
touch tests/__init__.py tests/conftest.py
```

## Project Structure (Cognita-inspired modular pattern)

```
backend/
├── app/
│   ├── __init__.py
│   ├── main.py              # FastAPI app entry
│   ├── config.py            # pydantic-settings singleton
│   ├── database.py          # Async engine & session
│   ├── constants.py         # App-wide constants
│   ├── models/              # SQLModel tables
│   │   ├── __init__.py      # Export all models
│   │   └── {entity}.py
│   ├── modules/             # Pluggable feature modules (Cognita pattern)
│   │   ├── __init__.py
│   │   └── {feature}/
│   │       ├── __init__.py
│   │       ├── router.py    # Feature routes
│   │       ├── service.py   # Business logic
│   │       └── schemas.py   # Request/response models
│   ├── api/
│   │   ├── __init__.py
│   │   ├── deps.py          # Dependency injection
│   │   └── v1/
│   │       ├── __init__.py
│   │       ├── router.py    # Aggregates all routes
│   │       └── {resource}.py
│   └── services/            # Shared business logic
│       ├── __init__.py
│       └── {domain}.py
├── tests/
│   ├── __init__.py
│   ├── conftest.py
│   └── api/
├── alembic/
│   ├── versions/
│   └── env.py
├── alembic.ini
├── pyproject.toml
└── Dockerfile
```

## Core Patterns

### config.py (Cognita-style singleton with validation)

```python
from pydantic import model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    """Application settings with validation (Cognita pattern)."""

    # Database
    database_url: str = "postgresql+asyncpg://app:secret@localhost:5432/app"

    # App
    debug: bool = False
    log_level: str = "info"
    secret_key: str = "change-me-in-production"

    # CORS
    cors_origins: list[str] = ["http://localhost:3000"]

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        extra="allow",  # Allow undefined env vars
    )

    @model_validator(mode="after")
    def validate_settings(self):
        """Validate settings after loading."""
        if not self.debug and self.secret_key == "change-me-in-production":
            raise ValueError("SECRET_KEY must be set in production")
        return self


# Singleton - instantiated once at module load
settings = Settings()
```

### main.py

```python
from contextlib import asynccontextmanager
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api.v1.router import api_router
from app.database import init_db

@asynccontextmanager
async def lifespan(app: FastAPI):
    await init_db()
    yield

app = FastAPI(
    title="API",
    version="1.0.0",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(api_router, prefix="/api/v1")
```

### database.py

```python
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker
from sqlmodel import SQLModel
from sqlmodel.ext.asyncio.session import AsyncSession

from app.config import settings

async_engine = create_async_engine(
    settings.database_url,
    pool_size=10,
    max_overflow=20,
    pool_pre_ping=True,
    pool_recycle=300,
    pool_use_lifo=True,
)

async_session = async_sessionmaker(
    bind=async_engine,
    class_=AsyncSession,
    expire_on_commit=False,
)

async def init_db():
    async with async_engine.begin() as conn:
        await conn.run_sync(SQLModel.metadata.create_all)

async def get_db():
    async with async_session() as session:
        yield session
```

### SQLModel Pattern (Base, Create, Public, Table)

```python
from datetime import datetime
from sqlmodel import Field, SQLModel, Relationship
from pydantic import field_validator

class ItemBase(SQLModel):
    """Shared fields for all Item schemas."""
    name: str = Field(min_length=1, max_length=100)
    quantity: int = Field(ge=0)

    @field_validator('name')
    @classmethod
    def validate_name(cls, v: str) -> str:
        return v.strip()

class ItemCreate(ItemBase):
    """Fields required to create an Item."""
    pass

class ItemUpdate(SQLModel):
    """Fields that can be updated (all optional)."""
    name: str | None = None
    quantity: int | None = None

class ItemPublic(ItemBase):
    """Fields returned to clients."""
    id: int
    created_at: datetime

class Item(ItemBase, table=True):
    """Database table definition."""
    id: int | None = Field(default=None, primary_key=True)
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    # Relationships
    location_id: int = Field(foreign_key="location.id")
    location: "Location" = Relationship(back_populates="items")
```

### Router Pattern

```python
from fastapi import APIRouter, Depends, HTTPException, status
from sqlmodel import select
from sqlmodel.ext.asyncio.session import AsyncSession

from app.api.deps import get_db
from app.models.item import Item, ItemCreate, ItemPublic, ItemUpdate

router = APIRouter(prefix="/items", tags=["items"])

@router.get("/", response_model=list[ItemPublic])
async def list_items(
    db: AsyncSession = Depends(get_db),
    limit: int = 20,
    offset: int = 0,
):
    """List all items with pagination."""
    stmt = select(Item).offset(offset).limit(limit)
    result = await db.execute(stmt)
    return result.scalars().all()

@router.post("/", response_model=ItemPublic, status_code=status.HTTP_201_CREATED)
async def create_item(
    item: ItemCreate,
    db: AsyncSession = Depends(get_db),
):
    """Create a new item."""
    db_item = Item.model_validate(item)
    db.add(db_item)
    await db.commit()
    await db.refresh(db_item)
    return db_item

@router.get("/{item_id}", response_model=ItemPublic)
async def get_item(
    item_id: int,
    db: AsyncSession = Depends(get_db),
):
    """Get a single item by ID."""
    item = await db.get(Item, item_id)
    if not item:
        raise HTTPException(status_code=404, detail="Item not found")
    return item

@router.patch("/{item_id}", response_model=ItemPublic)
async def update_item(
    item_id: int,
    item_update: ItemUpdate,
    db: AsyncSession = Depends(get_db),
):
    """Update an item."""
    item = await db.get(Item, item_id)
    if not item:
        raise HTTPException(status_code=404, detail="Item not found")

    update_data = item_update.model_dump(exclude_unset=True)
    for key, value in update_data.items():
        setattr(item, key, value)

    db.add(item)
    await db.commit()
    await db.refresh(item)
    return item

@router.delete("/{item_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_item(
    item_id: int,
    db: AsyncSession = Depends(get_db),
):
    """Delete an item."""
    item = await db.get(Item, item_id)
    if not item:
        raise HTTPException(status_code=404, detail="Item not found")
    await db.delete(item)
    await db.commit()
```

### Health Endpoints

```python
from fastapi import APIRouter, Depends
from sqlalchemy import text
from sqlmodel.ext.asyncio.session import AsyncSession

from app.api.deps import get_db

router = APIRouter(prefix="/health", tags=["health"])

@router.get("/live")
async def liveness():
    """Liveness probe - is the process running?"""
    return {"status": "ok"}

@router.get("/ready")
async def readiness(db: AsyncSession = Depends(get_db)):
    """Readiness probe - can we serve traffic?"""
    try:
        await db.execute(text("SELECT 1"))
        return {"status": "ok", "database": "connected"}
    except Exception as e:
        return {"status": "error", "database": str(e)}
```

### Feature Module Pattern (Cognita-inspired)

For complex features, organize as self-contained modules:

```
app/modules/inventory/
├── __init__.py          # Export router
├── router.py            # API endpoints
├── service.py           # Business logic
├── schemas.py           # Request/response models
└── dependencies.py      # Module-specific deps
```

```python
# app/modules/inventory/__init__.py
from .router import router

__all__ = ["router"]
```

```python
# app/modules/inventory/service.py
from sqlmodel.ext.asyncio.session import AsyncSession

class InventoryService:
    """Business logic for inventory operations."""

    def __init__(self, db: AsyncSession):
        self.db = db

    async def get_stock_level(self, item_id: int) -> int:
        """Calculate current stock from movements."""
        # Business logic here
        pass

    async def record_movement(self, item_id: int, quantity: int, type: str) -> None:
        """Record stock in/out movement."""
        # Business logic here
        pass
```

```python
# app/modules/inventory/router.py
from fastapi import APIRouter, Depends
from sqlmodel.ext.asyncio.session import AsyncSession

from app.api.deps import get_db
from .service import InventoryService

router = APIRouter(prefix="/inventory", tags=["inventory"])

def get_service(db: AsyncSession = Depends(get_db)) -> InventoryService:
    return InventoryService(db)

@router.get("/{item_id}/stock")
async def get_stock(
    item_id: int,
    service: InventoryService = Depends(get_service),
):
    return {"stock": await service.get_stock_level(item_id)}
```

```python
# app/main.py - Register module
from app.modules.inventory import router as inventory_router

app.include_router(inventory_router, prefix="/api/v1")
```

## See Also

- `ASYNC-PATTERNS.md` for advanced async patterns
- `AUTH-PATTERNS.md` for OAuth2/JWT implementation
- `PROJECT-STRUCTURE.md` for detailed file organization
