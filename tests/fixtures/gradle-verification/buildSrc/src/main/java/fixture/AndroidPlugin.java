package fixture;

import org.gradle.api.Plugin;
import org.gradle.api.Project;

/** Supplies AGP metadata without configuring an Android build. */
public final class AndroidPlugin implements Plugin<Project> {
    @Override
    public void apply(Project project) {}
}
