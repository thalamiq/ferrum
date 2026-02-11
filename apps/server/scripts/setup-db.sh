#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

DB_APP="zunder-db"
DB_NAME="fhir"

echo -e "${BLUE}Setting up Postgres database...${NC}\n"

# Check if database app exists
echo -e "${BLUE}[1/3] Checking database cluster...${NC}"
if ! fly status -a "$DB_APP" &>/dev/null; then
    echo -e "${RED}Error: Database app '$DB_APP' does not exist${NC}"
    echo -e "${YELLOW}Create it first:${NC}"
    echo -e "  fly postgres create --name $DB_APP --org thalamiq --region fra"
    exit 1
fi
echo -e "${GREEN}✓ Database cluster '$DB_APP' exists${NC}"

# Check cluster status
echo -e "\n${BLUE}[2/3] Checking cluster health...${NC}"
STATUS_OUTPUT=$(fly status -a "$DB_APP" 2>&1 || true)
if ! echo "$STATUS_OUTPUT" | grep -qi "running"; then
    echo -e "${YELLOW}Warning: Database cluster may not be running${NC}"
    echo -e "${YELLOW}Check status with: fly status -a $DB_APP${NC}"
else
    echo -e "${GREEN}✓ Cluster appears to be running${NC}"
fi

# Create database
echo -e "\n${BLUE}[3/3] Creating database '$DB_NAME'...${NC}"
CREATE_OUTPUT=$(fly postgres connect -a "$DB_APP" -c "CREATE DATABASE $DB_NAME;" 2>&1 || true)

if echo "$CREATE_OUTPUT" | grep -qi "already exists"; then
    echo -e "${YELLOW}✓ Database '$DB_NAME' already exists${NC}"
elif echo "$CREATE_OUTPUT" | grep -q "CREATE DATABASE"; then
    echo -e "${GREEN}✓ Database '$DB_NAME' created${NC}"
elif echo "$CREATE_OUTPUT" | grep -qi "no active leader found"; then
    echo -e "${RED}Error: No active leader found in Postgres cluster${NC}"
    echo -e "${YELLOW}The database cluster may be stopped or unhealthy.${NC}"
    echo -e "${YELLOW}Try:${NC}"
    echo -e "  fly status -a $DB_APP"
    echo -e "  fly postgres status -a $DB_APP"
    echo -e "\n${YELLOW}If the cluster is stopped, you may need to start it or create a new one.${NC}"
    exit 1
else
    echo -e "${RED}Warning: Unexpected output:${NC}"
    echo "$CREATE_OUTPUT"
fi

echo -e "\n${GREEN}✓ Database setup complete!${NC}"
echo -e "\n${YELLOW}Note: Migrations will run automatically when the app starts.${NC}"
echo -e "${YELLOW}The app detects if migrations are needed on startup.${NC}"
