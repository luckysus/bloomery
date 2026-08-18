import TrainingPanel from "../components/training/TrainingPanel";

type TrainingPageProps = Record<string, any>;

export default function TrainingPage(props: TrainingPageProps) {
  return <TrainingPanel {...(props as any)} />;
}
