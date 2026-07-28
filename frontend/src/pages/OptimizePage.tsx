import OptimizeWorkbench from "../components/optimize/OptimizeWorkbench";

type OptimizePageProps = Record<string, any>;

export default function OptimizePage(props: OptimizePageProps) {
  return <OptimizeWorkbench {...props} />;
}
