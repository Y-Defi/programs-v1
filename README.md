# Aril Program V1 Documentation

## Project Structure Overview

This project is a lending protocol based on the Anchor framework, designed with a modular architecture.

## Core Files Description

### Program Entry
- **lib.rs** - Main program entry file, defines the program ID and all public instruction interfaces

### Basic Modules

#### Error Handling
- **errors.rs** - Defines all custom error types, including math overflow, insufficient balance, low health factor, etc.

#### Utility Functions
- **utils.rs** - Common utility function module, containing core algorithms such as discount calculation, health factor calculation, etc.
- **math.rs** - Mathematical calculation module, handling complex calculations like LP valuation, precision conversion, overflow protection, etc.

### State Management Modules (state/)

#### Module Organization
- **mod.rs** - Unified entry point for state modules, re-exports all state structures

#### Core State Structures
- **protocol.rs** - Protocol global configuration, including collateral ratio, liquidation threshold, admin permissions, and other core parameters
- **position.rs** - User position state, recording user's LP deposits, USDC loans, health factor, etc.
- **pool.rs** - Fund pool state management, handling liquidity and lending pool data
- **liquidation.rs** - Liquidation-related state, managing liquidation records and remaining assets
- **traditional_usdc_pool.rs** - Traditional USDC fund pool state, supporting deposits, withdrawals, and interest calculations
- **constants.rs** - System constant definitions, including seeds, precision, limit values, etc.

### Instruction Modules (instructions/)

#### Module Organization
- **mod.rs** - Unified entry point for instruction modules, exports all instruction contexts and processing functions

#### Protocol Management
- **initialize_protocol.rs** - Protocol initialization, setting risk parameters and admin permissions

#### LP Token Operations
- **lp_operations.rs** - LP token deposit and withdrawal operations, supporting generic LP token validation and collateral management

#### Lending Operations
- **borrow_operations.rs** - USDC borrowing and repayment functionality, integrating health checks and interest calculations

#### Valuation System
- **valuation.rs** - Collateral value updates, real-time calculation of LP token value and health factor
- **raydium_integration.rs** - Raydium CLMM integration, obtaining pool data and price information
- **raydium_clmm_state.rs** - Raydium CLMM state structure definitions

#### Liquidation System (liquidation/)
- **mod.rs** - Unified exports for the liquidation module
- **liquidation_core.rs** - Core liquidation logic, handling liquidation state management and USDC allocation
- **liquidation_context.rs** - Account structure definitions for liquidation instructions
- **liquidation_validation.rs** - Health factor validation and liquidation condition checks

#### Traditional Finance Features
- **traditional_usdc_pool.rs** - Traditional USDC deposit and withdrawal pool, supporting interest calculation and liquidity management

#### Query Functions (query/)
- **mod.rs** - Unified exports for the query module
- **user_queries.rs** - User-related data queries
- **pool_queries.rs** - Fund pool state queries
- **admin_queries.rs** - Admin data queries
- **batch_queries.rs** - Batch data queries
- **interest_queries.rs** - Interest calculation queries

#### Admin Functions (admin/)
- **mod.rs** - Unified exports for the admin module
- **reset_liquidation_state.rs** - Reset user liquidation state
- **close_liquidation_record.rs** - Close liquidation records

## Core Functionality Module Details

### 1. Protocol Initialization System
Responsible for protocol startup configuration, including setting collateral ratio (50%), liquidation threshold (110%), admin permissions, and other core parameters.

### 2. LP Token Management System
- Supports generic LP token validation, not limited to specific AMMs
- Implements LP token deposit and withdrawal functionality
- Integrates with Raydium CLMM, obtaining real-time liquidity data

### 3. Lending Core System
- USDC borrowing functionality, calculating available loan amount based on collateral value
- Supports partial and full repayment
- Integrates locked interest rate mechanism

### 4. Valuation and Risk Management
- Real-time calculation of LP token value
- Integrates Chainlink oracle to obtain SOL price
- Dynamically calculates health factor, monitoring risk levels

### 5. Liquidation System
- Health monitoring, triggering liquidation when health factor falls below 110%
- Liquidation state management and remaining asset handling
- Supports external liquidator participation

### 6. Traditional Finance Features
- USDC deposit pool, providing liquidity for lending
- Dynamic interest rate calculation
- Deposit interest distribution

### 7. Query System
- Provides rich data query interfaces
- Supports user position queries, pool state queries
- Batch queries optimize performance

### 8. Admin Tools
- Protocol parameter adjustments
- Emergency state management
- Liquidation state reset
