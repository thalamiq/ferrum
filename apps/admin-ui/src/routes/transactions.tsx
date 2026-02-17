import { createRoute } from "@tanstack/react-router";
import TransactionDisplay from "@/components/TransactionDisplay";
import { rootRoute } from "./root";

function TransactionsPage() {
  return <TransactionDisplay />;
}

export const transactionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/transactions",
  component: TransactionsPage,
});
