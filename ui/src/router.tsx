// Code-based route tree (no file-based codegen — kept simple and fully
// readable in one file, which matters more than convention for a small,
// fixed set of routes). `Layout` is the root route's
// component, so the sidebar/topbar shell wraps every page via <Outlet/>.
import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import Layout from "./components/Layout";
import CertificateDetailPage from "./pages/CertificateDetail";
import CertificatesPage from "./pages/Certificates";
import ComponentsPage from "./pages/Components";
import DashboardPage from "./pages/Dashboard";
import ExecutionDetailPage from "./pages/ExecutionDetail";
import ExecutionQueuePage from "./pages/ExecutionQueue";
import ExecutionsPage from "./pages/Executions";
import TestPlanDetailPage from "./pages/TestPlanDetail";
import TestPlansPage from "./pages/TestPlans";
import NotFoundPage from "./pages/NotFound";
import SettingsPage from "./pages/Settings";

const rootRoute = createRootRoute({
  component: Layout,
});

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: DashboardPage,
});

const componentsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/components",
  component: ComponentsPage,
});

const testPlansRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/test-plans",
  validateSearch: (search: Record<string, unknown>) => ({
    dir: typeof search.dir === "string" ? search.dir : "",
    plan: typeof search.plan === "string" ? search.plan : undefined,
  }),
  component: TestPlansPage,
});

const testPlanEditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/test-plans/edit",
  validateSearch: (search: Record<string, unknown>) => ({
    path: typeof search.path === "string" ? search.path : "",
  }),
  component: TestPlanDetailPage,
});

const certificatesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/certificates",
  component: CertificatesPage,
});

const certificateDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/certificates/$certificateId",
  component: CertificateDetailPage,
});

const executionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/executions",
  component: ExecutionsPage,
});

const executionQueueRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/execution-queue",
  component: ExecutionQueuePage,
});

const executionDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/executions/$executionId",
  component: ExecutionDetailPage,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});

const routeTree = rootRoute.addChildren([
  dashboardRoute,
  testPlansRoute,
  testPlanEditRoute,
  certificatesRoute,
  certificateDetailRoute,
  executionQueueRoute,
  executionsRoute,
  executionDetailRoute,
  componentsRoute,
  settingsRoute,
]);

export const router = createRouter({
  routeTree,
  defaultNotFoundComponent: NotFoundPage,
});

// Registers the concrete router type globally so <Link to="…"> and
// useNavigate() are type-checked against the real route paths above.
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
