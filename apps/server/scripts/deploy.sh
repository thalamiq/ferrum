#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Configuration
APP_NAME="ferrum"
DB_APP="ferrum-db"
ORG="thalamiq"
REGION="fra"
REGISTRY="registry.fly.io"
IMAGE_NAME="${REGISTRY}/${APP_NAME}"

# Parse command line arguments
SKIP_BUILD=false
USE_REMOTE_BUILDER=false
SKIP_DB_CHECK=false

show_usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --skip-build       Skip Docker build (use existing local image)"
    echo "  --remote-build     Use Fly's remote builder instead of local build"
    echo "  --skip-db-check    Skip database setup verification"
    echo "  -h, --help         Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0                 # Full build and deploy"
    echo "  $0 --skip-build    # Deploy existing local image"
    echo "  $0 --remote-build  # Build remotely on Fly"
}

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --skip-build) SKIP_BUILD=true ;;
        --remote-build) USE_REMOTE_BUILDER=true ;;
        --skip-db-check) SKIP_DB_CHECK=false ;;
        -h|--help) show_usage; exit 0 ;;
        *) echo -e "${RED}Unknown parameter: $1${NC}"; show_usage; exit 1 ;;
    esac
    shift
done

echo -e "${BLUE}╔════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Deploying ${APP_NAME} to Fly.io${NC}     ${BLUE}║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════╝${NC}\n"

# Step 0: Pre-flight checks
echo -e "${BLUE}[Pre-flight] Checking prerequisites...${NC}"

# Check if app exists, create if needed
if ! fly status -a "${APP_NAME}" &>/dev/null; then
    echo -e "${YELLOW}→ App does not exist, creating in '${ORG}' organization...${NC}"
    fly apps create "${APP_NAME}" --org "${ORG}"
    echo -e "${GREEN}✓ App '${APP_NAME}' created${NC}"
else
    echo -e "${GREEN}✓ App '${APP_NAME}' exists${NC}"
fi

# Check database setup
if [ "$SKIP_DB_CHECK" = false ]; then
    echo -e "${BLUE}→ Checking database setup...${NC}"

    # Check if database app exists
    if ! fly status -a "${DB_APP}" &>/dev/null; then
        echo -e "${RED}✗ Database app '${DB_APP}' not found${NC}"
        echo -e "${YELLOW}Run: fly postgres create --name ${DB_APP} --org ${ORG} --region ${REGION}${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ Database cluster '${DB_APP}' exists${NC}"

    # Check if DATABASE_URL secret exists
    if ! fly secrets list -a "${APP_NAME}" 2>/dev/null | grep -q "DATABASE_URL"; then
        echo -e "${YELLOW}→ DATABASE_URL not set, attaching database...${NC}"
        if fly postgres attach "${DB_APP}" --app "${APP_NAME}" 2>&1 | grep -q "already contains a secret"; then
            echo -e "${GREEN}✓ DATABASE_URL already configured${NC}"
        else
            echo -e "${GREEN}✓ Database attached${NC}"
        fi
    else
        echo -e "${GREEN}✓ DATABASE_URL configured${NC}"
    fi
fi

echo ""

# Step 1: Build Docker image
STEP=1
if [ "$USE_REMOTE_BUILDER" = true ]; then
    echo -e "${YELLOW}[$STEP/4] Skipping local build (will use remote builder)${NC}"
elif [ "$SKIP_BUILD" = true ]; then
    echo -e "${YELLOW}[$STEP/4] Skipping build (using existing image)${NC}"
else
    echo -e "${BLUE}[$STEP/4] Building Docker image...${NC}"
    echo -e "${BLUE}→ Platform: linux/amd64${NC}"
    echo -e "${BLUE}→ Image: ${IMAGE_NAME}:latest${NC}"

    if docker build --platform linux/amd64 -t "${IMAGE_NAME}:latest" .; then
        echo -e "${GREEN}✓ Build complete${NC}"
    else
        echo -e "${RED}✗ Build failed${NC}"
        exit 1
    fi
fi
echo ""

# Step 2: Authenticate with Fly registry
STEP=2
echo -e "${BLUE}[$STEP/4] Authenticating with Fly registry...${NC}"
if fly auth docker &>/dev/null; then
    echo -e "${GREEN}✓ Authentication successful${NC}"
else
    echo -e "${RED}✗ Authentication failed${NC}"
    exit 1
fi
echo ""

# Step 3: Push image
STEP=3
if [ "$USE_REMOTE_BUILDER" = true ]; then
    echo -e "${YELLOW}[$STEP/4] Skipping image push (remote build)${NC}"
else
    echo -e "${BLUE}[$STEP/4] Pushing image to Fly registry...${NC}"
    echo -e "${BLUE}→ ${IMAGE_NAME}:latest${NC}"

    if docker push "${IMAGE_NAME}:latest"; then
        echo -e "${GREEN}✓ Push complete${NC}"
    else
        echo -e "${RED}✗ Push failed${NC}"
        exit 1
    fi
fi
echo ""

# Step 4: Deploy
STEP=4
echo -e "${BLUE}[$STEP/4] Deploying to Fly.io...${NC}"
if [ "$USE_REMOTE_BUILDER" = true ]; then
    echo -e "${BLUE}→ Using remote builder${NC}"
    fly deploy --remote-only --now
else
    echo -e "${BLUE}→ Using image: ${IMAGE_NAME}:latest${NC}"
    fly deploy --image "${IMAGE_NAME}:latest" --now
fi

echo ""
echo -e "${GREEN}╔════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║     Deployment Complete! 🚀            ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════╝${NC}\n"

# Post-deployment info
echo -e "${BLUE}📊 Status:${NC}"
fly status -a "${APP_NAME}" 2>/dev/null || echo -e "${YELLOW}Run: fly status -a ${APP_NAME}${NC}"

echo -e "\n${BLUE}📝 Useful commands:${NC}"
echo -e "  fly logs -a ${APP_NAME}                # View all logs"
echo -e "  fly logs -a ${APP_NAME} -n             # Stream logs"
echo -e "  fly status -a ${APP_NAME}              # Check app status"
echo -e "  fly ssh console -a ${APP_NAME}         # SSH into app"
echo -e "  fly postgres connect -a ${DB_APP}      # Connect to database"
echo -e "\n${BLUE}🌐 App URL:${NC} https://${APP_NAME}.fly.dev"
