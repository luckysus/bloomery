import ProfileCenter from "../components/profile/ProfileCenter";

type ProfilePageProps = Record<string, any>;

export default function ProfilePage(props: ProfilePageProps) {
  return <ProfileCenter {...(props as any)} />;
}
